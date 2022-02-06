use twilight_model::{
    application::command,
    id::{
        marker::{ChannelMarker, CommandVersionMarker, GenericMarker, RoleMarker, UserMarker},
        Id,
    },
};

pub use command_derive::ParseCommand;
pub use twilight_model::application::{
    command::{Command, CommandOption, CommandOptionType, CommandType},
    interaction::{application_command::CommandOptionValue, ApplicationCommand},
};

pub type Version = Id<CommandVersionMarker>;

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
    #[error("error parsing implicit option `{option}`: {error}")]
    ImplicitOption {
        option: &'static str,
        #[source]
        error: OptionError,
    },
    #[error("error parsing option `{option}`: {error}")]
    ExplicitOption {
        option: &'static str,
        #[source]
        error: OptionError,
    },
}

#[derive(Debug, thiserror::Error)]
pub enum OptionError {
    #[error("option missing")]
    Missing,
    #[error("incorrect type for option: expected `{expected:?}`, got `{actual:?}`")]
    InvalidType {
        expected: CommandOptionType,
        actual: CommandOptionType,
    },
}

pub trait ParseCommand: Sized {
    const NAME: &'static str;
    const DESCRIPTION: &'static str;
    const VERSION: Version;

    /// Generates a [`Command`] to send to Discord.
    fn command() -> Command;

    /// Parses an [`ApplicationCommand`] interaction.
    fn parse(command: ApplicationCommand) -> Result<Self, CommandError>;
}

pub trait ParseOption: Sized {
    const TYPE: CommandOptionType;

    fn option(meta: OptionMeta) -> CommandOption;

    fn parse(value: Option<&CommandOptionValue>) -> Result<Self, OptionError>;
}

pub struct OptionMeta {
    pub name: String,
    pub description: String,
    pub autocomplete: bool,
    pub required: bool,
    pub min_value: Option<command::CommandOptionValue>,
    pub max_value: Option<command::CommandOptionValue>,
}

macro_rules! impl_parse_option_base {
    ($($ty:ty),*: $kind:ident) => {
        $(
            impl ParseOption for $ty {
                const TYPE: CommandOptionType = CommandOptionType::$kind;

                fn option(meta: OptionMeta) -> CommandOption {
                    CommandOption::$kind(command::BaseCommandOptionData {
                        description: meta.description,
                        name: meta.name,
                        required: meta.required,
                    })
                }

                fn_parse_option!($ty: $kind);
            }
        )*
    }
}

macro_rules! fn_parse_option {
    ($ty:ty: $kind:ident) => {
        fn parse(value: Option<&CommandOptionValue>) -> Result<Self, OptionError> {
            match value {
                Some(CommandOptionValue::$kind(value)) => Ok(fn_parse_option!(@$kind(value)) as $ty),
                Some(value) => Err(OptionError::InvalidType {
                    expected: Self::TYPE,
                    actual: value.kind(),
                }),
                None => Err(OptionError::Missing),
            }
        }
    };
    (@Number($val:ident)) => { $val.0 };
    (@$_:ident($val:ident)) => { $val.clone() };
}

impl_parse_option_base!(Id<UserMarker>: User);
impl_parse_option_base!(Id<RoleMarker>: Role);
impl_parse_option_base!(Id<GenericMarker>: Mentionable);

macro_rules! impl_parse_option_number {
    ($($ty:ty),*: $kind:ident) => {
        $(
            impl ParseOption for $ty {
                const TYPE: CommandOptionType = CommandOptionType::$kind;

                fn option(meta: OptionMeta) -> CommandOption {
                    CommandOption::$kind(command::NumberCommandOptionData {
                        autocomplete: meta.autocomplete,
                        choices: Vec::new(),
                        description: meta.description,
                        max_value: meta.max_value,
                        min_value: meta.min_value,
                        name: meta.name,
                        required: meta.required,
                    })
                }

                fn_parse_option!($ty: $kind);
            }
        )*
    };
    (@Number($val:ident)) => { $val.0 };
    (@Integer($val:ident)) => { $val };
}
impl_parse_option_number!(f64, f32: Number);
impl_parse_option_number!(u8, i8, u16, i16, u32, i32, u64, i64, usize, isize: Integer);

impl ParseOption for String {
    const TYPE: CommandOptionType = CommandOptionType::String;

    fn option(meta: OptionMeta) -> CommandOption {
        CommandOption::String(command::ChoiceCommandOptionData {
            autocomplete: meta.autocomplete,
            choices: Vec::new(),
            description: meta.description,
            name: meta.name,
            required: meta.required,
        })
    }

    fn_parse_option!(String: String);
}

impl ParseOption for Id<ChannelMarker> {
    const TYPE: CommandOptionType = CommandOptionType::Channel;

    fn option(meta: OptionMeta) -> CommandOption {
        CommandOption::Channel(command::ChannelCommandOptionData {
            channel_types: Vec::new(),
            description: meta.description,
            name: meta.name,
            required: meta.required,
        })
    }

    fn_parse_option!(Id<ChannelMarker>: Channel);
}

impl<T> ParseOption for Option<T>
where
    T: ParseOption,
{
    const TYPE: CommandOptionType = T::TYPE;

    fn option(meta: OptionMeta) -> CommandOption {
        T::option(OptionMeta {
            required: false,
            ..meta
        })
    }

    fn parse(value: Option<&CommandOptionValue>) -> Result<Self, OptionError> {
        value.map(Some).map(T::parse).transpose()
    }
}

pub fn parse_user(command: &ApplicationCommand) -> Result<Id<UserMarker>, OptionError> {
    command
        .member
        .as_ref()
        .and_then(|mem| mem.user.as_ref())
        .or_else(|| command.user.as_ref())
        .map(|user| user.id)
        .ok_or(OptionError::Missing)
}
