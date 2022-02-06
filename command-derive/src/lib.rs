use convert_case::{Case, Casing};
use darling::{ast, Error, FromDeriveInput, FromField, FromMeta};
use proc_macro2::TokenStream;
use quote::quote;
use syn::parse_macro_input;

#[derive(FromDeriveInput)]
#[darling(attributes(command), supports(struct_any))]
struct StructAttrsRaw {
    ident: syn::Ident,
    /// Name of the command. Default to the identifier, translated to snake case.
    #[darling(default)]
    name: Option<String>,
    /// Version of the command. Defaults to 1.
    #[darling(default)]
    version: Option<u64>,
    /// Description of the command.
    #[darling(default)]
    description: String,
    data: ast::Data<(), OptionAttrsRaw>,
}

struct StructAttrs {
    ident: syn::Ident,
    name: String,
    version: u64,
    description: String,
    fields: StructFields,
}

impl FromDeriveInput for StructAttrs {
    fn from_derive_input(input: &syn::DeriveInput) -> Result<Self, Error> {
        let raw = StructAttrsRaw::from_derive_input(input)?;
        let mut errors = Vec::new();
        let ident = raw.ident;
        let name = raw
            .name
            .unwrap_or_else(|| ident.to_string().to_case(Case::Snake));
        let description = raw.description;
        if let Err(e) = validate_non_empty(&name, "name") {
            errors.push(e);
        }
        if let Err(e) = validate_non_empty(&description, "description") {
            errors.push(e);
        }
        let version = raw.version.unwrap_or(1);
        let fields: StructFields = raw
            .data
            .take_struct()
            .ok_or(Error::unsupported_shape("enum"))?
            .try_into()?;
        Ok(StructAttrs {
            ident,
            name,
            version,
            description,
            fields,
        })
    }
}

fn validate_non_empty(value: &str, name: &str) -> Result<(), Error> {
    if value.is_empty() {
        Err(Error::custom(format_args!(
            "'{name}' attrbute cannot be empty"
        )))
    } else {
        Ok(())
    }
}

enum StructFields {
    Unit,
    Tuple(Vec<TupleField>),
    Struct(Vec<StructField>),
}

impl TryFrom<darling::ast::Fields<OptionAttrsRaw>> for StructFields {
    type Error = Error;

    fn try_from(fields: darling::ast::Fields<OptionAttrsRaw>) -> Result<Self, Self::Error> {
        match fields.style {
            darling::ast::Style::Unit => {
                if !fields.fields.is_empty() {
                    Err(Error::custom("unexpected fields on unit struct"))
                } else {
                    Ok(StructFields::Unit)
                }
            }
            darling::ast::Style::Tuple => {
                let mut errors = Vec::new();
                let mut parsed_fields = Vec::with_capacity(fields.fields.len());
                for field in fields.fields {
                    if field.ident.is_some() {
                        errors.push(Error::custom("unexpected identifier on tuple struct field"));
                    }
                    match TupleField::try_from(field) {
                        Ok(field) => parsed_fields.push(field),
                        Err(error) => errors.push(error),
                    }
                }
                if errors.is_empty() {
                    Ok(StructFields::Tuple(parsed_fields))
                } else {
                    Err(Error::multiple(errors).flatten())
                }
            }
            darling::ast::Style::Struct => {
                let mut errors = Vec::new();
                let mut parsed_fields = Vec::with_capacity(fields.fields.len());
                for field in fields.fields {
                    match StructField::try_from(field) {
                        Ok(field) => parsed_fields.push(field),
                        Err(error) => errors.push(error),
                    }
                }
                if errors.is_empty() {
                    Ok(StructFields::Struct(parsed_fields))
                } else {
                    Err(Error::multiple(errors).flatten())
                }
            }
        }
    }
}

struct TupleField {
    name: String,
    ty: syn::Type,
    option: ParsedOption,
}

impl TupleField {
    fn to_command_option(&self) -> Option<TokenStream> {
        if let ParsedOption::Explicit(option) = &self.option {
            let ty = &self.ty;
            let name = &self.name;
            let description = &option.description;
            let autocomplete = option.autocomplete;
            let min_value = number_to_command_option_value(option.min);
            let max_value = number_to_command_option_value(option.max);
            Some(quote! {
                <#ty as ::command::ParseOption>::option(
                    ::command::OptionMeta {
                        name: #name.into(),
                        description: #description.into(),
                        autocomplete: #autocomplete,
                        required: true,
                        min_value: #min_value,
                        max_value: #max_value,
                    },
                )
            })
        } else {
            None
        }
    }

    fn to_get(&self) -> TokenStream {
        let name = &self.name;
        match &self.option {
            ParsedOption::Implicit(option) => {
                let implicit = &option.implicit;
                quote!(#implicit(&command).map_err(|error| {
                    ::command::CommandError::ImplicitOption {
                        option: #name,
                        error,
                    }
                })?)
            }
            ParsedOption::Explicit(_) => {
                let ty = &self.ty;
                quote! {
                    <#ty as ::command::ParseOption>::parse(
                        options.get(#name).copied()
                    )
                        .map_err(|error| {
                            ::command::CommandError::ExplicitOption {
                                option: #name,
                                error,
                            }
                        })?
                }
            }
        }
    }
}

impl TryFrom<OptionAttrsRaw> for TupleField {
    type Error = Error;

    fn try_from(field: OptionAttrsRaw) -> Result<Self, Self::Error> {
        let mut errors = Vec::new();
        let name = field.name.ok_or_else(|| Error::missing_field("name"))?;
        if let Err(e) = validate_non_empty(&name, "name") {
            errors.push(e);
        }
        let option = if let Some(implicit) = field.implicit {
            ParsedOption::Implicit(ImplicitOption { implicit })
        } else {
            let description = field
                .description
                .ok_or_else(|| Error::missing_field("description"))?;
            if let Err(e) = validate_non_empty(&description, "description") {
                errors.push(e);
            }
            ParsedOption::Explicit(ExplicitOption {
                description,
                autocomplete: field.autocomplete,
                min: field.min,
                max: field.max,
            })
        };
        if !errors.is_empty() {
            Err(Error::multiple(errors).flatten())
        } else {
            Ok(TupleField {
                name,
                ty: field.ty,
                option,
            })
        }
    }
}

struct StructField {
    ident: syn::Ident,
    field: TupleField,
}

impl TryFrom<OptionAttrsRaw> for StructField {
    type Error = Error;

    fn try_from(mut field: OptionAttrsRaw) -> Result<Self, Self::Error> {
        let ident = field
            .ident
            .clone()
            .ok_or_else(|| Error::custom("missing identifier for struct field"))?;
        if field.name.is_none() {
            field.name = Some(ident.to_string().to_case(Case::Snake));
        }
        Ok(StructField {
            ident,
            field: field.try_into()?,
        })
    }
}

enum ParsedOption {
    Implicit(ImplicitOption),
    Explicit(ExplicitOption),
}

struct ImplicitOption {
    implicit: syn::Path,
}

struct ExplicitOption {
    description: String,
    autocomplete: bool,
    min: Option<Number>,
    max: Option<Number>,
}

impl StructAttrs {
    fn consts(&self) -> TokenStream {
        let name = &self.name;
        let version = self.version;
        let description = &self.description;

        quote! {
            const NAME: &'static str = #name;
            const DESCRIPTION: &'static str = #description;
            const VERSION: ::command::Version = ::command::Version::new(#version);
        }
    }

    fn command(&self) -> TokenStream {
        let options = self.command_options();
        quote! {
            fn command() -> ::command::Command {
                ::command::Command {
                    application_id: None,
                    default_permission: None,
                    description: Self::DESCRIPTION.into(),
                    guild_id: None,
                    id: None,
                    kind: ::command::CommandType::ChatInput,
                    name: Self::NAME.into(),
                    options: ::std::vec![#(#options),*],
                    version: Self::VERSION,
                }
            }
        }
    }

    fn command_options(&self) -> Vec<TokenStream> {
        match &self.fields {
            StructFields::Unit => Vec::new(),
            StructFields::Tuple(fields) => fields
                .iter()
                .flat_map(TupleField::to_command_option)
                .collect(),
            StructFields::Struct(fields) => fields
                .iter()
                .map(|f| &f.field)
                .flat_map(TupleField::to_command_option)
                .collect(),
        }
    }

    fn parse(&self) -> TokenStream {
        let options = self.parse_options();
        quote! {
            fn parse(
                command: ::command::ApplicationCommand,
            ) -> Result<Self, ::command::CommandError> {
                let options = command
                    .data
                    .options
                    .iter()
                    .map(|opt| (&*opt.name, &opt.value))
                    .collect::<::std::collections::BTreeMap<_, _>>();
                Ok(#options)
            }
        }
    }

    fn parse_options(&self) -> TokenStream {
        match &self.fields {
            StructFields::Unit => quote!(Self),
            StructFields::Tuple(fields) => {
                let options = fields.iter().map(TupleField::to_get);
                quote!(Self(#(#options),*))
            }
            StructFields::Struct(fields) => {
                let options = fields.iter().map(|f| {
                    let ident = &f.ident;
                    let get = f.field.to_get();
                    quote!(#ident: #get)
                });
                quote!(Self { #(#options),* })
            }
        }
    }
}

#[derive(FromField)]
#[darling(attributes(command))]
struct OptionAttrsRaw {
    ident: Option<syn::Ident>,
    ty: syn::Type,
    #[darling(default)]
    name: Option<String>,
    #[darling(default)]
    description: Option<String>,
    #[darling(default)]
    implicit: Option<syn::Path>,
    #[darling(default)]
    autocomplete: bool,
    #[darling(default)]
    min: Option<Number>,
    #[darling(default)]
    max: Option<Number>,
}

#[derive(FromMeta, Clone, Copy)]
enum Number {
    I64(i64),
    F64(f64),
}

fn number_to_command_option_value(number: Option<Number>) -> TokenStream {
    let command_option_value = quote!(::command::CommandOptionValue);
    match number {
        Some(Number::I64(x)) => quote!(Some(#command_option_value::Integer(#x))),
        Some(Number::F64(x)) => quote!(Some(#command_option_value::Number(#x))),
        None => quote!(None),
    }
}

#[proc_macro_derive(ParseCommand, attributes(command))]
pub fn derive(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input = parse_macro_input!(input);
    let this = StructAttrs::from_derive_input(&input)
        .map_err(|e| eprintln!("{e}"))
        .unwrap();

    let ident = &this.ident;
    let consts = this.consts();
    let command = this.command();
    let parse = this.parse();

    let output = quote! {
        #[automatically_derived]
        impl ParseCommand for #ident {
            #consts
            #command
            #parse
        }
    };

    output.into()
}
