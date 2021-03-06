use std::collections::BTreeMap;
use std::sync::Arc;

use futures_util::StreamExt;
use tokio::sync::{Mutex, RwLock};
use twilight_gateway::{EventTypeFlags, Intents, Shard};
use twilight_http::{client::InteractionClient, Client};
use twilight_model::{
    application::{callback::InteractionResponse, command::Command, interaction::Interaction},
    gateway::event::Event,
    id::{marker::UserMarker, Id},
    oauth::current_application_info::CurrentApplicationInfo,
};
use twilight_util::builder::CallbackDataBuilder;

use crate::parser::{DoneCommand, TaskCommand, TodoCommand};

mod parser;

struct State {
    client: Client,
    application: CurrentApplicationInfo,
    db: RwLock<BTreeMap<Id<UserMarker>, Mutex<Vec<String>>>>,
    token: String,
}

impl State {
    async fn new() -> anyhow::Result<Arc<Self>> {
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

    fn interaction_client(&self) -> InteractionClient {
        self.client.interaction(self.application.id)
    }

    async fn init_commands(&self) -> anyhow::Result<()> {
        let commands: Vec<Command> =
            serde_yaml::from_reader(std::fs::File::open("commands.yaml")?)?;
        let get_commands = self
            .interaction_client()
            .set_global_commands(&commands)
            .exec()
            .await
            .map_err(pretty_error)?
            .models()
            .await?;

        log::info!(
            "registered commands: {:#}",
            serde_json::to_value(get_commands)?,
        );
        Ok(())
    }
}

#[tokio::main]
async fn main() {
    if let Err(e) = main_inner().await {
        eprintln!("{e:?}");
    }
}

async fn main_inner() -> anyhow::Result<()> {
    // Initialize the tracing subscriber.
    tracing_subscriber::fmt::init();

    let state = State::new().await?;
    state.init_commands().await?;

    let (shard, mut events) = Shard::builder(state.token.clone(), Intents::empty())
        .event_types(EventTypeFlags::INTERACTION_CREATE)
        .build();

    shard.start().await?;

    while let Some(event) = events.next().await {
        if let Event::InteractionCreate(interaction) = event {
            tokio::spawn(interaction_responder(Arc::clone(&state), interaction.0));
        }
    }

    Ok(())
}
async fn init_application(client: &Client) -> anyhow::Result<CurrentApplicationInfo> {
    let application = client
        .current_user_application()
        .exec()
        .await
        .map_err(pretty_error)?
        .model()
        .await?;

    Ok(application)
}

async fn interaction_responder(state: Arc<State>, interaction: Interaction) {
    if let Err(e) = interaction_responder_inner(state, interaction).await {
        log::error!("Error responding to interaction {e}\n{e:?}");
    }
}

async fn interaction_responder_inner(
    state: Arc<State>,
    interaction: Interaction,
) -> anyhow::Result<()> {
    match interaction {
        Interaction::ApplicationCommand(command) => {
            log::info!("command payload: {:#}", serde_json::to_value(&command)?);
            let interaction_id = command.id;
            let interaction_token = command.token.clone();
            let response = match TodoCommand::parse(*command)? {
                TodoCommand::Task(command) => handle_task(&state, command).await?,
                TodoCommand::Done(command) => handle_done(&state, command).await?,
            };
            log::info!("responding with response: {response:?}");
            state
                .interaction_client()
                .interaction_callback(interaction_id, &interaction_token, &response)
                .exec()
                .await?;
        }
        Interaction::ApplicationCommandAutocomplete(command) => {
            log::info!(
                "command autocomplete payload: {:#}",
                serde_json::to_value(command)?,
            );
        }
        _ => {}
    }
    Ok(())
}

async fn handle_task(state: &State, command: TaskCommand) -> anyhow::Result<InteractionResponse> {
    log::info!("handling task command: {command:?}");
    let idx = {
        let read_db = state.db.read().await;
        let mut write_db;
        let mut tasks = if let Some(tasks) = read_db.get(&command.user) {
            tasks.lock().await
        } else {
            drop(read_db);
            write_db = state.db.write().await;
            write_db.entry(command.user).or_default().lock().await
        };
        tasks.push(command.task.clone());
        tasks.len()
    };
    let cb = CallbackDataBuilder::new()
        .content(format!("Added \"{}\" at index {}", command.task, idx))
        .build();
    Ok(InteractionResponse::ChannelMessageWithSource(cb))
}
async fn handle_done(_state: &State, _command: DoneCommand) -> anyhow::Result<InteractionResponse> {
    todo!();
}

fn pretty_error(e: twilight_http::Error) -> anyhow::Error {
    use twilight_http::error::ErrorType;
    if let ErrorType::Response {
        body,
        error,
        status,
    } = e.kind()
    {
        let data = if let Ok(data) = serde_json::from_slice::<serde_json::Value>(body) {
            data
        } else {
            serde_json::Value::String(String::from_utf8_lossy(body).into())
        };
        anyhow::anyhow!("error: {error}\nstatus: {status}\nbody: {data:#}")
    } else {
        e.into()
    }
}
