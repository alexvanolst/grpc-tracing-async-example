use hello_world::greeter_server::{Greeter, GreeterServer};
use hello_world::{HelloReply, HelloRequest};
use opentelemetry::sdk::propagation::TraceContextPropagator;
use opentelemetry::trace::TraceContextExt;
use opentelemetry::Context;
use opentelemetry::{global, propagation::Extractor};
use std::collections::HashMap;
use tonic::metadata::KeyRef;
use tonic::{transport::Server, Request, Response, Status};
use tracing::*;
use tracing_opentelemetry::OpenTelemetrySpanExt;
use tracing_subscriber::prelude::*;
use tracing_subscriber::{fmt, registry};

#[allow(clippy::derive_partial_eq_without_eq)]
pub mod hello_world {
    tonic::include_proto!("helloworld");
}

struct MetadataMap<'a>(&'a HashMap<String, String>);

impl<'a> Extractor for MetadataMap<'a> {
    /// Get a value for a key from the MetadataMap.  If the value can't be converted to &str, returns None
    fn get(&self, key: &str) -> Option<&str> {
        let x = self.0.get(key);

        x.map(|x| &**x)
    }

    /// Collect all the keys from the MetadataMap.
    fn keys(&self) -> Vec<&str> {
        self.0.keys().map(|x| &**x).collect()
    }
}

#[instrument(skip_all)]
fn expensive_fn(to_print: String, ctx: Context) {
    tracing::Span::current().set_parent(ctx);
    tracing::warn!("Expensive task info:");
    tracing::warn!(span_span_id = ?Span::current().context().span().span_context().span_id());
    tracing::warn!(span_trace_id = ?Span::current().context().span().span_context().trace_id(), "pre_run");
    tracing::warn!(ctx_span_id = ?Context::current().span().span_context().span_id());
    std::thread::sleep(std::time::Duration::from_millis(100));
    inner_expensive();
    info!("Inside expensive {}", to_print);
}

#[instrument]
fn inner_expensive() {
    std::thread::sleep(std::time::Duration::from_millis(500));
}

#[derive(Debug, Default)]
pub struct MyGreeter {}

#[tonic::async_trait]
impl Greeter for MyGreeter {
    #[instrument(skip_all)]
    async fn say_hello(
        &self,
        request: Request<HelloRequest>, // Accept request of type HelloRequest
    ) -> Result<Response<HelloReply>, Status> {
        for key in request.metadata().keys() {
            match key {
                KeyRef::Ascii(x) => {
                    let mut opt = request.metadata().get(x).map(|v| v.to_str().unwrap());
                    let value = opt.get_or_insert("no value");
                    tracing::info!("Got metadata at {:?}: {}", key, value)
                }
                KeyRef::Binary(_) => {}
            }
        }
        let context_map = HashMap::new(); // TODO

        let parent_cx =
            global::get_text_map_propagator(|prop| prop.extract(&MetadataMap(&context_map)));
        tracing::Span::current().set_parent(parent_cx.clone());
        tracing::warn!("Sync info:");
        tracing::warn!(span_span_id = ?Span::current().context().span().span_context().span_id());
        tracing::warn!(span_trace_id = ?Span::current().context().span().span_context().trace_id(), "pre_run");
        tracing::warn!(ctx_span_id = ?Context::current().span().span_context().span_id());

        let name = request.into_inner().name;
        let current = Span::current().context();
        let fork_name = name.clone();
        let _jh =
            tokio::spawn(
                async move { expensive_fn(format!("Got name: {:?}", fork_name), current) },
            );
        // Return an instance of type HelloReply
        let reply = hello_world::HelloReply {
            message: format!("Hello {}!", name),
        };

        Ok(Response::new(reply))
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
    global::set_text_map_propagator(TraceContextPropagator::new());

    let subscriber = registry().with(fmt::layer().without_time().with_target(false).boxed());

    let tracer = opentelemetry_jaeger::new_agent_pipeline()
        .with_service_name("grpc-server")
        .install_batch(opentelemetry::runtime::Tokio)?;
    subscriber
        .with(tracing_subscriber::EnvFilter::new("INFO"))
        .with(tracing_opentelemetry::layer().with_tracer(tracer))
        .try_init()?;

    let addr = "[::1]:50051".parse()?;
    let greeter = MyGreeter::default();

    Server::builder()
        .add_service(GreeterServer::new(greeter))
        .serve(addr)
        .await?;

    opentelemetry::global::shutdown_tracer_provider();

    Ok(())
}
