use hello_world::greeter_client::GreeterClient;
use hello_world::HelloRequest;
use opentelemetry::sdk::propagation::TraceContextPropagator;
use opentelemetry::trace::TraceContextExt;
use opentelemetry::Context;
use opentelemetry::{global, propagation::Injector};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::*;
use tracing_opentelemetry::OpenTelemetrySpanExt;
use tracing_subscriber::prelude::*;
use tracing_subscriber::{fmt, registry};

#[derive(Serialize, Deserialize)]
struct OwnedMetadataMap(HashMap<String, String>);

impl Default for OwnedMetadataMap {
    fn default() -> Self {
        OwnedMetadataMap(HashMap::new())
    }
}

impl<'a> Injector for OwnedMetadataMap {
    /// Set a key and value in the MetadataMap.  Does nothing if the key or value are not valid inputs
    fn set(&mut self, key: &str, value: String) {
        self.0.insert(key.into(), value);
    }
}

#[allow(clippy::derive_partial_eq_without_eq)] // tonic don't derive Eq for generated types. We shouldn't manually change it.
pub mod hello_world {
    tonic::include_proto!("helloworld");
}

async fn greet() -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
    let span = tracing::info_span!("root span");
    let _span_raii = span.enter();
    tracing::warn!(span_span_id = ?span.context().span().span_context().span_id(), "pre_run");
    tracing::warn!(span_trace_id = ?span.context().span().span_context().trace_id(), "pre_run");
    tracing::warn!(ctx_span_id = ?Context::current().span().span_context().span_id(), "pre_run");

    let mut client = GreeterClient::connect("http://[::1]:50051").await?;

    let mut owned_metadata = OwnedMetadataMap::default();

    global::get_text_map_propagator(|propagator| {
        propagator.inject_context(&span.context(), &mut owned_metadata)
    });
    let inner_span = info_span!("make_grpc_request");
    inner_span.set_parent(span.context());

    tracing::warn!("client say hello");
    tracing::warn!(span_span_id = ?inner_span.context().span().span_context().span_id(), "pre_run");
    tracing::warn!(span_trace_id = ?inner_span.context().span().span_context().trace_id(), "pre_run");
    tracing::warn!(ctx_span_id = ?inner_span.context().span().span_context().span_id(), "pre_run");

    let oof = serde_json::to_string(&owned_metadata);
    let request = tonic::Request::new(HelloRequest { name: oof.unwrap() });

    let response = client.say_hello(request).instrument(inner_span).await?;

    info!("Response received: {:?}", response);
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
    global::set_text_map_propagator(TraceContextPropagator::new());

    let subscriber = registry().with(fmt::layer().without_time().with_target(false).boxed());

    let tracer = opentelemetry_jaeger::new_agent_pipeline()
        .with_service_name("grpc-client")
        .install_simple()?;
    subscriber
        .with(tracing_subscriber::EnvFilter::new("INFO"))
        .with(tracing_opentelemetry::layer().with_tracer(tracer))
        .try_init()?;

    greet().await?;

    opentelemetry::global::shutdown_tracer_provider();

    Ok(())
}
