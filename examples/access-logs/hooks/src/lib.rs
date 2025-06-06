mod access_logs;

use access_logs::{AuditInfo, OperationInfo, SubgraphInfo};
use grafbase_hooks::{grafbase_hooks, host_io::access_log, CacheStatus, Hooks, SharedContext};

struct Component;

#[grafbase_hooks]
impl Hooks for Component {
    fn new() -> Self
    where
        Self: Sized,
    {
        Self
    }

    fn on_subgraph_response(&mut self, _: SharedContext, request: grafbase_hooks::ExecutedSubgraphRequest) -> Vec<u8> {
        // One response per subgraph execution.
        let responses = request.executions.into_iter().map(Into::into).collect();

        let info = SubgraphInfo {
            subgraph_name: &request.subgraph_name,
            method: &request.method,
            url: &request.url,
            responses,
            total_duration_ms: request.total_duration_ms,
            has_errors: request.has_errors,
            cached: matches!(request.cache_status, CacheStatus::Hit),
        };

        // This is not written to the logs, so we use postcard crate for faster serialization.
        postcard::to_stdvec(&info).unwrap()
    }

    fn on_operation_response(&mut self, _: SharedContext, request: grafbase_hooks::ExecutedOperation) -> Vec<u8> {
        let info = OperationInfo {
            name: request.name.as_deref(),
            document: &request.document,
            prepare_duration_ms: request.prepare_duration_ms,
            cached_plan: request.cached_plan,
            duration_ms: request.duration_ms,
            status: request.status.into(),
            // Every return value from on_subgraph_response hook is returned here.
            subgraphs: request
                .on_subgraph_response_outputs
                .iter()
                // Again, using postgraph so we can deserialize faster than serde_json.
                .filter_map(|bytes| postcard::from_bytes(bytes).ok())
                .collect(),
        };

        // This is not written to the logs, so we use postcard crate for faster serialization.
        postcard::to_stdvec(&info).unwrap()
    }

    fn on_http_response(&mut self, context: SharedContext, request: grafbase_hooks::ExecutedHttpRequest) {
        let info = AuditInfo {
            method: &request.method,
            url: &request.url,
            status_code: request.status_code,
            // Trace ID is only available if the gateway opentelemetry setting is enabled.
            trace_id: &context.trace_id(),
            // Every return value from on_operation_response hook is returned here.
            operations: request
                .on_operation_response_outputs
                .iter()
                .filter_map(|bytes| postcard::from_bytes(bytes).ok())
                .collect(),
        };

        // Writes to the access log file. The last serialization step is with serde_json, so the output
        // is JSON. Only use JSON when expecting a JSON result, and use postcard to speed up the intermediate
        // steps.
        //
        // We calculated utilizing postcard in the previous steps takes about 150 us or 15% off per request.
        access_log::send(&serde_json::to_vec(&info).unwrap()).unwrap()
    }
}

grafbase_hooks::register_hooks!(Component);
