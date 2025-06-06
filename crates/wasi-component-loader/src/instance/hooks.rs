use std::str::FromStr;

use anyhow::anyhow;
use enumflags2::BitFlags;
use http::HeaderMap;
use runtime::hooks::SubgraphRequest;
use wasmtime::component::{ComponentNamedList, Lift, Lower, Resource, TypedFunc};

use crate::AccessLogSender;
use crate::extension::api::wit;
use crate::extension::api::wit::Error;
use crate::headers::HookHeaders;
use crate::names::{
    AUTHORIZE_EDGE_NODE_POST_EXECUTION_HOOK_FUNCTION, AUTHORIZE_EDGE_POST_EXECUTION_HOOK_FUNCTION,
    AUTHORIZE_EDGE_PRE_EXECUTION_HOOK_FUNCTION, AUTHORIZE_NODE_PRE_EXECUTION_HOOK_FUNCTION,
    AUTHORIZE_PARENT_EDGE_POST_EXECUTION_HOOK_FUNCTION, GATEWAY_HOOK_FUNCTION, INIT_HOOKS_FUNCTION,
    ON_HTTP_RESPONSE_FUNCTION, ON_OPERATION_RESPONSE_FUNCTION, ON_SUBGRAGH_REQUEST_HOOK_FUNCTION,
    ON_SUBGRAPH_RESPONSE_FUNCTION,
};
use crate::{ComponentLoader, SharedContext};
use crate::{
    ContextMap, EdgeDefinition, ExecutedHttpRequest, ExecutedOperation, ExecutedSubgraphRequest, GuestResult,
    NodeDefinition,
};

use super::ComponentInstance;

pub(crate) mod authorization;
pub(crate) mod response;

/// An enum representing the different hook implementations that can be called by the guest.
#[enumflags2::bitflags]
#[repr(u32)]
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum HookImplementation {
    /// The `authorize_edge_pre_execution` hook implementation.
    AuthorizeEdgePreExecution = 1 << 0,
    /// The `authorize_node_pre_execution` hook implementation.
    AuthorizeNodePreExecution = 1 << 1,
    /// The `authorize_parent_edge_post_execution` hook implementation.
    AuthorizeParentEdgePostExecution = 1 << 2,
    /// The `authorize_edge_node_post_execution` hook implementation.
    AuthorizeEdgeNodePostExecution = 1 << 3,
    /// The `authorize_edge_post_execution` hook implementation.
    AuthorizeEdgePostExecution = 1 << 4,
    /// The `on_gateway_request` hook implementation.
    OnGatewayRequest = 1 << 5,
    /// The `on_subgraph_response` hook implementation.
    OnSubgraphResponse = 1 << 6,
    /// The `on_operation_response` hook implementation.
    OnOperationResponse = 1 << 7,
    /// The `on_http_response` hook implementation.
    OnHttpResponse = 1 << 8,
    /// The `on_subgraph_request` hook implementation.
    OnSubgraphRequest = 1 << 9,
}

impl HookImplementation {
    /// The name of the hook function.
    pub fn name(&self) -> &'static str {
        match self {
            HookImplementation::AuthorizeEdgePreExecution => AUTHORIZE_EDGE_PRE_EXECUTION_HOOK_FUNCTION,
            HookImplementation::AuthorizeNodePreExecution => AUTHORIZE_NODE_PRE_EXECUTION_HOOK_FUNCTION,
            HookImplementation::AuthorizeParentEdgePostExecution => AUTHORIZE_PARENT_EDGE_POST_EXECUTION_HOOK_FUNCTION,
            HookImplementation::AuthorizeEdgeNodePostExecution => AUTHORIZE_EDGE_NODE_POST_EXECUTION_HOOK_FUNCTION,
            HookImplementation::AuthorizeEdgePostExecution => AUTHORIZE_EDGE_POST_EXECUTION_HOOK_FUNCTION,
            HookImplementation::OnGatewayRequest => GATEWAY_HOOK_FUNCTION,
            HookImplementation::OnSubgraphResponse => ON_SUBGRAPH_RESPONSE_FUNCTION,
            HookImplementation::OnOperationResponse => ON_OPERATION_RESPONSE_FUNCTION,
            HookImplementation::OnHttpResponse => ON_HTTP_RESPONSE_FUNCTION,
            HookImplementation::OnSubgraphRequest => ON_SUBGRAGH_REQUEST_HOOK_FUNCTION,
        }
    }
}

impl FromStr for HookImplementation {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s == AUTHORIZE_EDGE_PRE_EXECUTION_HOOK_FUNCTION {
            Ok(HookImplementation::AuthorizeEdgePreExecution)
        } else if s == AUTHORIZE_NODE_PRE_EXECUTION_HOOK_FUNCTION {
            Ok(HookImplementation::AuthorizeNodePreExecution)
        } else if s == AUTHORIZE_PARENT_EDGE_POST_EXECUTION_HOOK_FUNCTION {
            Ok(HookImplementation::AuthorizeParentEdgePostExecution)
        } else if s == AUTHORIZE_EDGE_NODE_POST_EXECUTION_HOOK_FUNCTION {
            Ok(HookImplementation::AuthorizeEdgeNodePostExecution)
        } else if s == AUTHORIZE_EDGE_POST_EXECUTION_HOOK_FUNCTION {
            Ok(HookImplementation::AuthorizeEdgePostExecution)
        } else if s == GATEWAY_HOOK_FUNCTION {
            Ok(HookImplementation::OnGatewayRequest)
        } else if s == ON_SUBGRAPH_RESPONSE_FUNCTION {
            Ok(HookImplementation::OnSubgraphResponse)
        } else if s == ON_OPERATION_RESPONSE_FUNCTION {
            Ok(HookImplementation::OnOperationResponse)
        } else if s == ON_HTTP_RESPONSE_FUNCTION {
            Ok(HookImplementation::OnHttpResponse)
        } else if s == ON_SUBGRAGH_REQUEST_HOOK_FUNCTION {
            Ok(HookImplementation::OnSubgraphRequest)
        } else {
            Err(anyhow!("Unknown hook function: {}", s))
        }
    }
}

/// An instance of a hooks component.
pub struct HooksComponentInstance {
    component: ComponentInstance,
    hooks: BitFlags<HookImplementation>,
}

impl HooksComponentInstance {
    /// Creates a new hooks component instance.
    ///
    /// # Arguments
    ///
    /// * `loader` - A reference to the `ComponentLoader` used to load the component.
    /// * `interface_name` - The name of the interface this component implements.
    ///
    /// # Returns
    ///
    /// A `Result` containing the newly created component instance on success, or an error on failure.
    pub async fn new(loader: &ComponentLoader, access_log: AccessLogSender) -> crate::Result<Self> {
        let mut component = ComponentInstance::new(loader, access_log).await?;

        let init = component
            .get_typed_func::<(), (i64,)>(INIT_HOOKS_FUNCTION)
            .ok_or_else(|| anyhow!("init-hooks function not found"))?;

        let (bits,) = init.call_async(component.store_mut(), ()).await?;
        init.post_return_async(component.store_mut()).await?;

        Ok(Self {
            component,
            hooks: BitFlags::<HookImplementation>::from_bits(bits as u32).unwrap(),
        })
    }

    /// Returns the info on implemented hooks.
    pub fn hooks_implemented(&self) -> BitFlags<HookImplementation> {
        self.hooks
    }

    /// Called just before parsing and executing a gateway operation.
    ///
    /// # Arguments
    ///
    /// * `context` - A map containing the key-value context store for the request.
    /// * `headers` - A map containing the request headers.
    ///
    /// # Returns
    ///
    /// Returns a result containing a tuple of the processed context and headers,
    /// or an error if the operation fails.
    pub async fn on_gateway_request(
        &mut self,
        context: ContextMap,
        url: &str,
        headers: HeaderMap,
    ) -> crate::GatewayResult<(ContextMap, HeaderMap)> {
        let Some(hook) = self.get_hook::<_, (Result<(), wit::ErrorResponse>,)>(HookImplementation::OnGatewayRequest)
        else {
            return Ok((context, headers));
        };

        // adds the data to the shared memory
        let context = self.component.store_mut().data_mut().push_resource(context)?;
        let headers = self
            .component
            .store_mut()
            .data_mut()
            .push_resource(HookHeaders::borrow(headers))?;

        // we need to take the pointers now, because a resource is not Copy and we need
        // the pointers to get the data back from the shared memory.
        let headers_rep = headers.rep();
        let context_rep = context.rep();

        let result = hook
            .call_async(self.component.store_mut(), (context, url, headers))
            .await;

        if result.is_err() {
            self.component.poison();
        } else {
            hook.post_return_async(self.component.store_mut()).await?;
        }

        result?.0?;

        // take the data back from the shared memory
        let context = self.component.store_mut().data_mut().take_resource(context_rep)?;
        let headers: HookHeaders = self.component.store_mut().data_mut().take_resource(headers_rep)?;

        Ok((context, headers.into_owned().unwrap()))
    }

    /// A hook called just before executing a subgraph request.
    ///
    /// # Arguments
    ///
    /// * `context` - A shared context for the request.
    /// * `subgraph_name` - The name of the subgraph being requested.
    /// * `method` - The HTTP method of the request (e.g., GET, POST).
    /// * `url` - The URL for the subgraph request.
    /// * `headers` - The headers associated with the subgraph request.
    ///
    /// # Returns
    ///
    /// Returns a result containing the headers if the subgraph request should continue, or an
    /// error if the execution should abort.
    pub async fn on_subgraph_request(
        &mut self,
        context: SharedContext,
        subgraph_name: &str,
        request: SubgraphRequest,
    ) -> crate::Result<SubgraphRequest> {
        let Some(hook) = self.get_hook::<_, (GuestResult<()>,)>(HookImplementation::OnSubgraphRequest) else {
            return Ok(request);
        };

        let subgraph_name = subgraph_name.to_string();

        // adds the data to the shared memory
        let context = self.component.store_mut().data_mut().push_resource(context)?;
        let request = self.component.store_mut().data_mut().push_resource(request)?;

        // we need to take the pointers now, because a resource is not Copy and we need
        // the pointers to get the data back from the shared memory.
        let request_rep = request.rep();
        let context_rep = context.rep();

        let result = hook
            .call_async(self.component.store_mut(), (context, subgraph_name, request))
            .await;

        if result.is_err() {
            self.component.poison();
        } else {
            hook.post_return_async(self.component.store_mut()).await?;
        }

        result?.0?;

        // take the data back from the shared memory
        self.component
            .store_mut()
            .data_mut()
            .take_resource::<SharedContext>(context_rep)?;

        let request = self.component.store_mut().data_mut().take_resource(request_rep)?;

        Ok(request)
    }

    /// Calls the pre authorize hook for an edge.
    ///
    /// This function is invoked before the execution of an edge operation. It checks
    /// whether the operation is authorized based on the provided `definition`, `arguments`,
    /// and `metadata`. If the authorization check fails, an error is returned.
    ///
    /// # Arguments
    ///
    /// - `context`: The shared context for the operation.
    /// - `definition`: The edge definition containing type and field names.
    /// - `arguments`: A string representing the arguments for the operation.
    /// - `metadata`: A string containing metadata for the operation.
    ///
    /// # Returns
    ///
    /// Returns a `Result` indicating success or failure of the authorization check.
    pub async fn authorize_edge_pre_execution(
        &mut self,
        context: SharedContext,
        definition: EdgeDefinition,
        arguments: String,
        metadata: String,
    ) -> crate::Result<()> {
        self.call3_one_output(
            HookImplementation::AuthorizeEdgePreExecution,
            context,
            (definition, arguments, metadata),
        )
        .await?
        .map(|result: GuestResult<()>| result.map_err(Into::into))
        .ok_or_else(|| {
            crate::Error::from(format!(
                "{AUTHORIZE_EDGE_PRE_EXECUTION_HOOK_FUNCTION} hook must be defined if using the @authorized directive"
            ))
        })?
    }

    /// Calls the pre authorize hook for a node.
    ///
    /// This function is invoked before the execution of a node operation. It checks
    /// whether the operation is authorized based on the provided `definition` and
    /// `metadata`. If the authorization check fails, an error is returned.
    ///
    /// # Arguments
    ///
    /// - `context`: The shared context for the operation.
    /// - `definition`: The node definition containing the type name.
    /// - `metadata`: A string containing metadata for the operation.
    ///
    /// # Returns
    ///
    /// Returns a `Result` indicating success or failure of the authorization check.
    pub async fn authorize_node_pre_execution(
        &mut self,
        context: SharedContext,
        definition: NodeDefinition,
        metadata: String,
    ) -> crate::Result<()> {
        self.call2_one_output(
            HookImplementation::AuthorizeNodePreExecution,
            context,
            (definition, metadata),
        )
        .await?
        .map(|result: GuestResult<()>| result.map_err(Into::into))
        .ok_or_else(|| {
            crate::Error::from(format!(
                "{AUTHORIZE_EDGE_PRE_EXECUTION_HOOK_FUNCTION} hook must be defined if using the @authorized directive"
            ))
        })?
    }

    /// Calls the post authorize hook for a parent edge.
    ///
    /// This function is invoked after the execution of a parent edge operation. It checks
    /// whether the operation is authorized based on the provided `definition`, `parents`,
    /// and `metadata`. If the authorization check fails, an error is returned.
    ///
    /// # Arguments
    ///
    /// - `context`: The shared context for the operation.
    /// - `definition`: The edge definition containing type and field names.
    /// - `parents`: A vector of strings representing the parent nodes for the operation.
    /// - `metadata`: A string containing metadata for the operation.
    ///
    /// # Returns
    ///
    /// Returns a `Result` containing a vector of results indicating success or failure
    /// of the authorization checks for each parent node.
    pub async fn authorize_parent_edge_post_execution(
        &mut self,
        context: SharedContext,
        definition: EdgeDefinition,
        parents: Vec<String>,
        metadata: String,
    ) -> crate::Result<Vec<Result<(), Error>>> {
        self.call3_one_output(
            HookImplementation::AuthorizeParentEdgePostExecution,
            context,
            (definition, parents, metadata),
        )
        .await?
        .map(|result: Vec<GuestResult<()>>| Ok(result))
        .ok_or_else(|| {
            crate::Error::from(format!(
                "{AUTHORIZE_PARENT_EDGE_POST_EXECUTION_HOOK_FUNCTION} hook must be defined if using the @authorized directive"
            ))
        })?
    }

    /// Calls the post authorize hook for an edge involving nodes.
    ///
    /// This function is invoked after the execution of an edge operation involving
    /// nodes. It checks whether the operation is authorized based on the provided
    /// `definition`, `nodes`, and `metadata`. If the authorization check fails,
    /// an error is returned.
    ///
    /// # Parameters
    ///
    /// - `context`: The shared context for the operation.
    /// - `definition`: The edge definition containing type and field names.
    /// - `nodes`: A vector of strings representing the nodes for the operation.
    /// - `metadata`: A string containing metadata for the operation.
    ///
    /// # Returns
    ///
    /// Returns a `Result` containing a vector of results indicating success or
    /// failure of the authorization checks for each node.
    pub async fn authorize_edge_node_post_execution(
        &mut self,
        context: SharedContext,
        definition: EdgeDefinition,
        nodes: Vec<String>,
        metadata: String,
    ) -> crate::Result<Vec<Result<(), Error>>> {
        self.call3_one_output(
            HookImplementation::AuthorizeEdgeNodePostExecution,
            context,
            (definition, nodes, metadata),
        )
        .await?
        .map(|result: Vec<GuestResult<()>>| Ok(result))
        .ok_or_else(|| {
            crate::Error::from(format!(
                "{AUTHORIZE_EDGE_NODE_POST_EXECUTION_HOOK_FUNCTION} hook must be defined if using the @authorized directive"
            ))
        })?
    }

    /// Calls the post authorize hook for an edge.
    ///
    /// This function is invoked after the execution of an edge operation. It checks
    /// whether the operation is authorized based on the provided `definition`, `edges`,
    /// and `metadata`. If the authorization check fails, an error is returned.
    ///
    /// # Arguments
    ///
    /// - `context`: The shared context for the operation.
    /// - `definition`: The edge definition containing type and field names.
    /// - `edges`: A vector of tuples where each tuple contains a string representing an edge
    ///   and a vector of strings representing associated nodes for the operation.
    /// - `metadata`: A string containing metadata for the operation.
    ///
    /// # Returns
    ///
    /// Returns a `Result` containing a vector of results indicating success or failure
    /// of the authorization checks for each edge.
    pub async fn authorize_edge_post_execution(
        &mut self,
        context: SharedContext,
        definition: EdgeDefinition,
        edges: Vec<(String, Vec<String>)>,
        metadata: String,
    ) -> crate::Result<Vec<Result<(), Error>>> {
        self.call3_one_output(
            HookImplementation::AuthorizeEdgePostExecution,
            context,
            (definition, edges, metadata),
        )
        .await?
        .map(|result: Vec<GuestResult<()>>| Ok(result))
        .ok_or_else(|| {
            crate::Error::from(format!(
                "{AUTHORIZE_EDGE_POST_EXECUTION_HOOK_FUNCTION} hook must be defined if using the @authorized directive"
            ))
        })?
    }

    /// Allows inspection of the response from a subgraph request.
    ///
    /// # Arguments
    ///
    /// * `context` - A shared context for the operation.
    /// * `request` - The executed subgraph request containing details of the request.
    ///
    /// # Returns
    ///
    /// A `Result` containing a serialized vector of bytes from the user on success,
    /// or an error on failure.
    pub async fn on_subgraph_response(
        &mut self,
        context: SharedContext,
        request: ExecutedSubgraphRequest,
    ) -> crate::Result<Vec<u8>> {
        self.call1_one_output(HookImplementation::OnSubgraphResponse, context, request)
            .await?
            .map(|result: Vec<u8>| Ok(result))
            .unwrap_or_else(|| Ok(Vec::new()))
    }

    /// Allows inspection of the response from an executed operation.
    ///
    /// # Arguments
    ///
    /// * `context` - A shared context for the operation.
    /// * `request` - The executed operation containing details of the operation.
    ///
    /// # Returns
    ///
    /// A `Result` containing a serialized vector of bytes from the user on success,
    /// or an error on failure.
    pub async fn on_operation_response(
        &mut self,
        context: SharedContext,
        request: ExecutedOperation,
    ) -> crate::Result<Vec<u8>> {
        self.call1_one_output(HookImplementation::OnOperationResponse, context, request)
            .await?
            .map(|result: Vec<u8>| Ok(result))
            .unwrap_or_else(|| Ok(Vec::new()))
    }

    /// Allows inspection of the response from an executed HTTP request.
    ///
    /// # Arguments
    ///
    /// * `context` - A shared context for the operation.
    /// * `request` - The executed HTTP request containing details of the request.
    pub async fn on_http_response(
        &mut self,
        context: SharedContext,
        request: ExecutedHttpRequest,
    ) -> crate::Result<()> {
        self.call1_without_output(HookImplementation::OnHttpResponse, context, request)
            .await
    }

    /// Calls a function with one input argument and no output.
    ///
    /// # Type Parameters
    ///
    /// * `A1` - The type of the first argument.
    ///
    /// # Arguments
    ///
    /// * `name` - The name of the function to call.
    /// * `context` - A shared context resource.
    /// * `arg` - The first argument to pass to the function.
    ///
    /// # Returns
    ///
    /// A `Result` indicating success or failure. If the function call is successful, it returns `Ok(())`.
    async fn call1_without_output<A1>(
        &mut self,
        instance: HookImplementation,
        context: SharedContext,
        arg: A1,
    ) -> crate::Result<()>
    where
        (Resource<SharedContext>, A1): ComponentNamedList + Lower + Send + Sync + 'static,
    {
        let Some(hook) = self.get_hook::<(Resource<SharedContext>, A1), ()>(instance) else {
            return Ok(());
        };

        let context = self.component.store_mut().data_mut().push_resource(context)?;
        let context_rep = context.rep();

        let result = hook.call_async(self.component.store_mut(), (context, arg)).await;

        // We check if the hook call trapped, and if so we mark the instance poisoned.
        //
        // If no traps, we mark this hook so it can be called again.
        if result.is_err() {
            self.component.poison();
        } else {
            hook.post_return_async(self.component.store_mut()).await?;
        }

        result?;

        // This is a bit ugly because we don't need it, but we need to clean the shared
        // resources before exiting or this will leak RAM.
        let _: SharedContext = self.component.store_mut().data_mut().take_resource(context_rep)?;

        Ok(())
    }

    /// Calls a function with one input argument and one output.
    ///
    /// # Type Parameters
    ///
    /// * `A1` - The type of the first argument.
    /// * `R` - The type of the output.
    ///
    /// # Arguments
    ///
    /// * `name` - The name of the function to call.
    /// * `context` - A shared context resource.
    /// * `arg` - The first argument to pass to the function.
    ///
    /// # Returns
    ///
    /// A `Result` containing an `Option<R>`. If the function call is successful, it returns `Ok(Some(result))`,
    /// where `result` is the output of the function. If the function call fails, it returns an error. If the
    /// function does not exist, it returns `Ok(None)`.
    async fn call1_one_output<A1, R>(
        &mut self,
        instance: HookImplementation,
        context: SharedContext,
        arg: A1,
    ) -> crate::Result<Option<R>>
    where
        (Resource<SharedContext>, A1): ComponentNamedList + Lower + Send + Sync + 'static,
        (R,): ComponentNamedList + Lift + Send + Sync + 'static,
    {
        let Some(hook) = self.get_hook::<(Resource<SharedContext>, A1), (R,)>(instance) else {
            return Ok(None);
        };

        let context = self.component.store_mut().data_mut().push_resource(context)?;
        let context_rep = context.rep();

        let result = hook.call_async(self.component.store_mut(), (context, arg)).await;

        // We check if the hook call trapped, and if so we mark the instance poisoned.
        //
        // If no traps, we mark this hook so it can be called again.
        if result.is_err() {
            self.component.poison();
        } else {
            hook.post_return_async(self.component.store_mut()).await?;
        }

        let result = result?.0;

        // This is a bit ugly because we don't need it, but we need to clean the shared
        // resources before exiting or this will leak RAM.
        let _: SharedContext = self.component.store_mut().data_mut().take_resource(context_rep)?;

        Ok(Some(result))
    }

    /// Calls a function with two input arguments and one output.
    ///
    /// # Type Parameters
    ///
    /// * `A1` - The type of the first argument.
    /// * `A2` - The type of the second argument.
    /// * `R` - The type of the output.
    ///
    /// # Arguments
    ///
    /// * `name` - The name of the function to call.
    /// * `context` - A shared context resource.
    /// * `args` - A tuple containing the two arguments to pass to the function.
    ///
    /// # Returns
    ///
    /// A `Result` containing an `Option<R>`. If the function call is successful, it returns `Ok(Some(result))`,
    /// where `result` is the output of the function. If the function call fails, it returns an error. If the
    /// function does not exist, it returns `Ok(None)`.
    async fn call2_one_output<A1, A2, R>(
        &mut self,
        implementation: HookImplementation,
        context: SharedContext,
        args: (A1, A2),
    ) -> crate::Result<Option<R>>
    where
        (Resource<SharedContext>, A1, A2): ComponentNamedList + Lower + Send + Sync + 'static,
        (R,): ComponentNamedList + Lift + Send + Sync + 'static,
    {
        let Some(hook) = self.get_hook::<(Resource<SharedContext>, A1, A2), (R,)>(implementation) else {
            return Ok(None);
        };

        let context = self.component.store_mut().data_mut().push_resource(context)?;
        let context_rep = context.rep();

        let result = hook
            .call_async(self.component.store_mut(), (context, args.0, args.1))
            .await;

        // We check if the hook call trapped, and if so we mark the instance poisoned.
        //
        // If no traps, we mark this hook so it can be called again.
        if result.is_err() {
            self.component.poison();
        } else {
            hook.post_return_async(self.component.store_mut()).await?;
        }

        let result = result?.0;

        // This is a bit ugly because we don't need it, but we need to clean the shared
        // resources before exiting or this will leak RAM.
        let _: SharedContext = self.component.store_mut().data_mut().take_resource(context_rep)?;

        Ok(Some(result))
    }

    /// Calls a function with three input arguments and one output.
    ///
    /// # Type Parameters
    ///
    /// * `A1` - The type of the first argument.
    /// * `A2` - The type of the second argument.
    /// * `A3` - The type of the third argument.
    /// * `R` - The type of the output.
    ///
    /// # Arguments
    ///
    /// * `name` - The name of the function to call.
    /// * `context` - A shared context resource.
    /// * `args` - A tuple containing the three arguments to pass to the function.
    ///
    /// # Returns
    ///
    /// A `Result` containing an `Option<R>`. If the function call is successful, it returns `Ok(Some(result))`,
    /// where `result` is the output of the function. If the function call fails, it returns an error. If the
    /// function does not exist, it returns `Ok(None)`.
    async fn call3_one_output<A1, A2, A3, R>(
        &mut self,
        implementation: HookImplementation,
        context: SharedContext,
        args: (A1, A2, A3),
    ) -> crate::Result<Option<R>>
    where
        (Resource<SharedContext>, A1, A2, A3): ComponentNamedList + Lower + Send + Sync + 'static,
        (R,): ComponentNamedList + Lift + Send + Sync + 'static,
    {
        let Some(hook) = self.get_hook::<(Resource<SharedContext>, A1, A2, A3), (R,)>(implementation) else {
            return Ok(None);
        };

        let context = self.component.store_mut().data_mut().push_resource(context)?;
        let context_rep = context.rep();

        let result = hook
            .call_async(self.component.store_mut(), (context, args.0, args.1, args.2))
            .await;

        // We check if the hook call trapped, and if so we mark the instance poisoned.
        //
        // If no traps, we mark this hook so it can be called again.
        if result.is_err() {
            self.component.poison();
        } else {
            hook.post_return_async(self.component.store_mut()).await?;
        }

        let result = result?.0;

        // This is a bit ugly because we don't need it, but we need to clean the shared
        // resources before exiting or this will leak RAM.
        let _: SharedContext = self.component.store_mut().data_mut().take_resource(context_rep)?;

        Ok(Some(result))
    }

    /// Retrieves a typed function (hook) by its name from the component instance.
    ///
    /// # Type Parameters
    ///
    /// * `I` - The input type for the function.
    /// * `O` - The output type for the function.
    ///
    /// # Arguments
    ///
    /// * `function_name` - The name of the function to retrieve.
    ///
    /// # Returns
    ///
    /// An `Option<TypedFunc<I, O>>`, which is `Some` if the function was found and can be cast to the expected types,
    /// or `None` if the function does not exist or could not be retrieved.
    fn get_hook<I, O>(&mut self, hook: HookImplementation) -> Option<TypedFunc<I, O>>
    where
        I: ComponentNamedList + Lower + Send + Sync + 'static,
        O: ComponentNamedList + Lift + Send + Sync + 'static,
    {
        if !self.hooks.contains(hook) {
            return None;
        }

        self.component.get_typed_func(hook.name())
    }

    /// Checks if the instance can be recycled.
    ///
    /// # Returns
    ///
    /// A `Result` indicating success or failure. On success, it returns `Ok(())`.
    /// On failure, it returns an error if the instance is poisoned.
    pub fn recycle(&mut self) -> crate::Result<()> {
        if self.component.poisoned() {
            return Err(anyhow!("this instance is poisoned").into());
        }

        Ok(())
    }
}
