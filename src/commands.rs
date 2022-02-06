use command::{ApplicationCommand, Error, ParseCommand};
use twilight_model::id::{marker::UserMarker, Id};

lazy_static::lazy_static! {
    pub static ref COMMANDS: Vec<command::Command> = vec![
        TaskCommand::command(),
        DoneCommand::command(),
    ];
}

#[derive(Debug)]
pub enum TodoCommand {
    Task(TaskCommand),
    Done(DoneCommand),
}

impl TodoCommand {
    pub fn parse(command: ApplicationCommand) -> Result<Self, Error> {
        match &*command.data.name {
            TaskCommand::NAME => {
                TaskCommand::parse(command)
                    .map(TodoCommand::Task)
                    .map_err(|error| Error::CommandError {
                        command: TaskCommand::NAME,
                        error,
                    })
            }
            DoneCommand::NAME => {
                DoneCommand::parse(command)
                    .map(TodoCommand::Done)
                    .map_err(|error| Error::CommandError {
                        command: DoneCommand::NAME,
                        error,
                    })
            }
            _ => Err(Error::InvalidCommand(command.data.name)),
        }
    }
}

#[derive(ParseCommand, Debug)]
#[command(
    name = "task",
    version = 2,
    description = "Add a task to the todo list"
)]
pub struct TaskCommand {
    #[command(implicit = "command::parse_user")]
    pub user: Id<UserMarker>,
    #[command(description = "the task to create")]
    pub task: String,
}

#[derive(ParseCommand, Debug)]
#[command(name = "done", version = 1, description = "Mark a task as done")]
pub struct DoneCommand {
    #[command(implicit = "command::parse_user")]
    pub user: Id<UserMarker>,
}
