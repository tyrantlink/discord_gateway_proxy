use twilight_gateway::{Message, Shard, ShardId, Intents};
use ed25519_dalek::{Signer, SigningKey};
use time::OffsetDateTime;
use serde::Deserialize;
use std::error::Error;
use std::fs;
use hex;

#[derive(Deserialize)]
struct Project {
    bot_token: String,
    api_url: String,
    signing_key: String,
}

async fn load_project() -> Result<Project, Box<dyn Error>> {
    let project_str: String = fs::read_to_string("project.toml")?;
    let project: Project = toml::from_str(&project_str)?;
    Ok(project)
}

fn create_signature(
    signing_key: &SigningKey,
    timestamp: &str,
    body: &str,
) -> String {
    hex::encode(
        signing_key.sign(
            format!("{}{}", timestamp, body)
            .as_bytes()
        ).to_bytes()
    )
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let project: Project = load_project().await?;

    let signing_key: SigningKey = SigningKey::from_bytes(
        &hex::decode(&project.signing_key)?.try_into().unwrap());

    let intents: Intents = Intents::GUILD_MESSAGES | Intents::GUILD_MESSAGE_REACTIONS | Intents::MESSAGE_CONTENT;
    let mut shard: Shard = Shard::new(ShardId::ONE, project.bot_token, intents);

    let client = reqwest::Client::new();
    let api_endpoint: String = format!("{}/discord/event", project.api_url.trim_end_matches('/'));

    println!("event forwarding to {}", api_endpoint);

    loop {
        let message: Message = match shard.next_message().await {
            Ok(message) => message,
            Err(_) => {
                continue;
            }
        };

        let json_str: String = match message {
            Message::Text(content) => content,
            Message::Close(_) => {
                continue;
            }
        };

        if json_str.starts_with("{\"t\":\"READY\"") || json_str.starts_with("{\"t\":null") {
            continue;
        }

        tokio::spawn(forward_event(
            client.clone(),
            api_endpoint.clone(),
            json_str,
            signing_key.clone(),
        ));
    }
}

async fn forward_event(
    client: reqwest::Client,
    endpoint: String,
    json_str: String,
    signing_key: SigningKey,
) {
    let timestamp = OffsetDateTime::now_utc()
    .format(&time::format_description::well_known::Rfc3339)
    .unwrap();

    let signature = create_signature(&signing_key, &timestamp, &json_str);

    match client.post(&endpoint)
        .header("Content-Type", "application/json")
        .header("X-Signature-Ed25519", signature)
        .header("X-Signature-Timestamp", timestamp)
        .body(json_str)
        .send()
        .await
    {
        Ok(response) => {
            if !response.status().is_success() {
                println!("api request failed with status: {}", response.status());
            } else {
                println!("forwarded {} event", response.text().await.unwrap());
            }
        }
        Err(e) => println!("error forwarding event: {:?}", e),
    }
}
