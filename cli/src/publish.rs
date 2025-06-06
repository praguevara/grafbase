use crate::{api, cli_input::PublishCommand, errors::CliError, output::report};
use std::{
    fs,
    io::{IsTerminal, Read},
};

const FAILED_PUBLISH_STATUS: i32 = 1;

#[tokio::main]
pub(crate) async fn publish(
    PublishCommand {
        subgraph_name,
        graph_ref,
        source,
        schema_path,
        message,
        ..
    }: PublishCommand,
) -> Result<(), CliError> {
    let schema = match schema_path {
        Some(path) => fs::read_to_string(path).map_err(CliError::SchemaReadError)?,
        None if std::io::stdin().is_terminal() => {
            return Err(CliError::MissingArgument("--schema or a schema piped through stdin"));
        }
        None => {
            let mut schema = String::new();

            std::io::stdin()
                .read_to_string(&mut schema)
                .map_err(CliError::SchemaReadError)?;

            schema
        }
    };

    report::publishing();

    let outcome = api::publish::publish(
        graph_ref.account(),
        graph_ref.graph(),
        graph_ref.branch(),
        &subgraph_name,
        source.url.as_ref().map(|url| url.as_str()),
        &schema,
        message.as_deref(),
    )
    .await
    .map_err(CliError::BackendApiError)?;

    match &outcome {
        api::publish::PublishOutcome::Success { composition_errors } if composition_errors.is_empty() => {
            report::publish_command_success(&subgraph_name);
        }
        api::publish::PublishOutcome::Success { composition_errors } => {
            report::publish_command_composition_failure(composition_errors);
        }
        api::publish::PublishOutcome::NoChange => report::publish_no_change(),
        api::publish::PublishOutcome::GraphDoesNotExist {
            account_slug,
            graph_slug,
        } => {
            report::publish_graph_does_not_exist(account_slug, graph_slug);
            std::process::exit(FAILED_PUBLISH_STATUS);
        }
    };

    Ok(())
}
