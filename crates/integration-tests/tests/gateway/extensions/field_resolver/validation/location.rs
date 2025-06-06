use integration_tests::{gateway::Gateway, runtime};

use super::EchoExt;

#[test]
fn invalid_location() {
    runtime().block_on(async move {
        // Invalid field directive
        let result = Gateway::builder()
            .with_subgraph_sdl(
                "a",
                r#"
                extend schema
                    @link(url: "echo-1.0.0", import: ["@echo", "@meta"])

                scalar JSON

                type Query {
                    echo: JSON @meta
                }
                "#,
            )
            .with_extension(EchoExt::with_sdl(
                r#"
                directive @meta on SCHEMA
                directive @echo on FIELD_DEFINITION
            "#,
            ))
            .try_build()
            .await;

        insta::assert_snapshot!(result.unwrap_err(), @r#"
        At site Query.echo, extension echo-1.0.0 directive @meta used in the wrong location FIELD_DEFINITION, expected one of: SCHEMA
        See schema at 19:35:
        (graph: A, extension: ECHO, name: "meta", arguments: {})
        "#);

        // Invalid schema directive
        let result = Gateway::builder()
            .with_subgraph_sdl(
                "a",
                r#"
                extend schema
                    @link(url: "echo-1.0.0", import: ["@echo", "@meta"])
                    @echo

                scalar JSON

                type Query {
                    echo: JSON
                }
                "#,
            )
            .with_extension(EchoExt::with_sdl(
                r#"
                directive @meta on SCHEMA
                directive @echo on FIELD_DEFINITION
                "#,
            ))
            .try_build()
            .await;

        insta::assert_snapshot!(result.unwrap_err(), @r#"
        At site subgraph named 'a', extension echo-1.0.0 directive @echo used in the wrong location SCHEMA, expected one of: FIELD_DEFINITION
        See schema at 29:97:
        {graph: A, name: "echo", arguments: {}}
        "#);
    });
}
