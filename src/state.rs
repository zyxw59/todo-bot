use std::collections::BTreeMap;
use std::sync::Arc;

use tokio::sync::{Mutex, RwLock};
use twilight_http::{client::InteractionClient, Client};
use twilight_model::{
    id::{marker::UserMarker, Id},
    oauth::current_application_info::CurrentApplicationInfo,
};

pub struct State {
    pub client: Client,
    pub application: CurrentApplicationInfo,
    pub db: RwLock<BTreeMap<Id<UserMarker>, Mutex<Vec<String>>>>,
    pub token: String,
}

impl State {
    pub async fn new() -> anyhow::Result<Arc<Self>> {
        let token = std::fs::read_to_string("token")?.trim().to_owned();
        let client = Client::new(token.clone());
        let application = init_application(&client).await?;

        Ok(Arc::new(State {
            client,
            application,
            token,
            db: RwLock::new(BTreeMap::new()),
        }))
    }

    pub fn interaction_client(&self) -> InteractionClient {
        self.client.interaction(self.application.id)
    }

    pub async fn init_commands(&self) -> anyhow::Result<()> {
        let get_commands = self
            .interaction_client()
            .set_global_commands(&crate::commands::COMMANDS)
            .exec()
            .await
            .map_err(crate::pretty_error)?
            .models()
            .await?;

        log::info!(
            "registered commands: {:#}",
            serde_json::to_value(get_commands)?,
        );
        Ok(())
    }
}

async fn init_application(client: &Client) -> anyhow::Result<CurrentApplicationInfo> {
    let application = client
        .current_user_application()
        .exec()
        .await
        .map_err(crate::pretty_error)?
        .model()
        .await?;

    Ok(application)
}
