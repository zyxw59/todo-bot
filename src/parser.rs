use twilight_model::{
    application::{
        command::CommandOptionType,
        interaction::{application_command::CommandOptionValue, ApplicationCommand},
    },
    id::{marker::UserMarker, Id},
};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Invalid command: `{0}`")]
    InvalidCommand(String),
    #[error("Failed to parse `{command}` command: {error}")]
    CommandError {
        command: &'static str,
        #[source]
        error: CommandError,
    },
}

#[derive(Debug, thiserror::Error)]
pub enum CommandError {
    #[error("missing user")]
    MissingUser,
    #[error("missing the `{0}` option")]
    MissingOption(&'static str),
    #[error("`{option}` is of wrong type: expected `{expected:?}`, got `{actual:?}`")]
    InvalidType {
        option: &'static str,
        expected: CommandOptionType,
        actual: CommandOptionType,
    },
}

#[derive(Debug)]
pub enum TodoCommand {
    Task(TaskCommand),
    Done(DoneCommand),
}

impl TodoCommand {
    pub fn parse(command: ApplicationCommand) -> Result<Self, Error> {
        match &*command.data.name {
            TaskCommand::COMMAND => TaskCommand::parse(command).map(TodoCommand::Task),
            DoneCommand::COMMAND => DoneCommand::parse(command).map(TodoCommand::Done),
            _ => Err(Error::InvalidCommand(command.data.name)),
        }
    }
}

#[derive(Debug)]
pub struct TaskCommand {
    pub user: Id<UserMarker>,
    pub task: String,
}

impl TaskCommand {
    const COMMAND: &'static str = "task";

    fn parse(command: ApplicationCommand) -> Result<Self, Error> {
        Self::parse_inner(command).map_err(|error| Error::CommandError {
            command: Self::COMMAND,
            error,
        })
    }

    fn parse_inner(command: ApplicationCommand) -> Result<Self, CommandError> {
        let user = command
            .member
            .and_then(|mem| mem.user)
            .ok_or(CommandError::MissingUser)?
            .id;
        let task = command
            .data
            .options
            .into_iter()
            .find(|opt| opt.name == "task")
            .ok_or(CommandError::MissingOption("task"))?
            .value
            .to_string()
            .map_err(|val| CommandError::InvalidType {
                option: "task",
                expected: CommandOptionType::String,
                actual: val.kind(),
            })?;
        Ok(TaskCommand { user, task })
    }
}

#[derive(Debug)]
pub struct DoneCommand;

impl DoneCommand {
    const COMMAND: &'static str = "done";

    fn parse(_command: ApplicationCommand) -> Result<Self, Error> {
        Ok(DoneCommand)
    }
}

trait CommandOptionValueExt: Sized {
    fn to_string(self) -> Result<String, Self>;
}

impl CommandOptionValueExt for CommandOptionValue {
    fn to_string(self) -> Result<String, Self> {
        match self {
            CommandOptionValue::String(string) => Ok(string),
            _ => Err(self),
        }
    }
}
