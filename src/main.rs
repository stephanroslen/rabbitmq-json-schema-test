use futures::StreamExt;
use rabbitmq_stream_client::{
    Environment,
    types::{ByteCapacity, Message, OffsetSpecification},
};
use tracing::level_filters::LevelFilter;
use tracing_subscriber::FmtSubscriber;
use typify::import_types;

import_types!(schema = "schema.json");

async fn producer(environment: Environment, stream_name: &str, num: usize) -> anyhow::Result<()> {
    let producer = environment.producer().build(stream_name).await?;
    for i in 0..num {
        let msg = Something {
            some_number: i as f64,
            some_string: format!("Number {}", i),
        };
        producer
            .send_with_confirm(
                Message::builder()
                    .body(serde_json::to_string(&msg)?)
                    .build(),
            )
            .await?;
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    producer.close().await?;

    tracing::info!("Producer finished");

    Ok(())
}

async fn consumer(environment: Environment, stream_name: &str, num: usize) -> anyhow::Result<()> {
    let mut consumer = environment
        .consumer()
        .offset(OffsetSpecification::First)
        .build(stream_name)
        .await?;
    for _ in 0..num {
        let delivery = consumer
            .next()
            .await
            .ok_or(anyhow::anyhow!("No message received"))??;
        let received: Something = serde_json::from_slice(
            delivery
                .message()
                .data()
                .ok_or(anyhow::anyhow!("No message data"))?,
        )?;

        tracing::info!("Received: {:?}", received);
    }
    consumer.handle().close().await?;

    tracing::info!("Consumer finished");

    Ok(())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let subscriber = FmtSubscriber::builder()
        .with_max_level(LevelFilter::INFO)
        .finish();

    tracing::subscriber::set_global_default(subscriber)?;

    let environment = Environment::builder().build().await?;

    let stream_name = "some-stream";
    let num = 10_usize;

    tracing::info!("Creating stream {}", stream_name);

    environment
        .stream_creator()
        .max_length(ByteCapacity::KB(5))
        .create(stream_name)
        .await?;

    tracing::info!("Starting consumer and producer");

    let handles = [
        tokio::spawn(consumer(environment.clone(), stream_name, num)),
        tokio::spawn(producer(environment.clone(), stream_name, num)),
    ];

    futures::future::try_join_all(handles.into_iter()).await?;

    tracing::info!("Deleting stream {}", stream_name);

    environment.delete_stream(stream_name).await?;

    Ok(())
}
