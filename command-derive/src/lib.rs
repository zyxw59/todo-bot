use convert_case::{Case, Casing};
use darling::{ast, FromDeriveInput, FromField, FromMeta};
use proc_macro2::TokenStream;
use quote::quote;
use syn::parse_macro_input;

#[derive(FromDeriveInput)]
#[darling(attributes(command), supports(struct_any))]
struct StructAttrs {
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
    data: ast::Data<(), OptionAttrs>,
}

impl StructAttrs {
    fn consts(&self) -> TokenStream {
        let ident = &self.ident;
        let name = self
            .name
            .clone()
            .unwrap_or_else(|| ident.to_string().to_case(Case::Snake));
        let version = self.version.unwrap_or(1);
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
        if let ast::Data::Struct(fields) = &self.data {
            let mut options = Vec::with_capacity(fields.fields.len());
            for option in &fields.fields {
                if let OptionAttrs::Explicit(option) = option {
                    let ty = &option.ty;
                    let name = option.name();
                    let description = &option.description;
                    let autocomplete = option.autocomplete;
                    let min_value = number_to_command_option_value(option.min);
                    let max_value = number_to_command_option_value(option.max);
                    options.push(quote! {
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
                    });
                }
            }
            options
        } else {
            panic!("Expected struct");
        }
    }

    fn parse(&self) -> TokenStream {
        let options = self.parse_options();
        let init = if let ast::Data::Struct(fields) = &self.data {
            match fields.style {
                darling::ast::Style::Tuple => quote!((#(#options),*)),
                darling::ast::Style::Struct => quote!({ #(#options),* }),
                darling::ast::Style::Unit => quote!(),
            }
        } else {
            panic!("Expected struct");
        };
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
                Ok(Self #init)
            }
        }
    }

    fn parse_options(&self) -> Vec<TokenStream> {
        if let ast::Data::Struct(fields) = &self.data {
            let mut options = Vec::with_capacity(fields.fields.len());
            for option in &fields.fields {
                let name = option.name();
                let get = match option {
                    OptionAttrs::Implicit(option) => {
                        let implicit = &option.implicit;
                        quote!(#implicit(&command).map_err(|error| {
                            ::command::CommandError::ImplicitOption {
                                option: #name,
                                error,
                            }
                        })?)
                    }
                    OptionAttrs::Explicit(option) => {
                        let ty = &option.ty;
                        quote! {
                            <#ty as ::command::ParseOption>::parse(options.get(#name).copied())
                                .map_err(|error| {
                                    ::command::CommandError::ExplicitOption {
                                        option: #name,
                                        error,
                                    }
                                })?
                        }
                    }
                };
                if let Some(ident) = option.ident() {
                    options.push(quote!(#ident: #get));
                } else {
                    options.push(get);
                }
            }
            options
        } else {
            panic!("Expected struct");
        }
    }
}

enum OptionAttrs {
    Implicit(ImplicitOptionAttrs),
    Explicit(ExplicitOptionAttrs),
}

impl OptionAttrs {
    fn ident(&self) -> Option<&syn::Ident> {
        match self {
            OptionAttrs::Implicit(option) => option.ident.as_ref(),
            OptionAttrs::Explicit(option) => option.ident.as_ref(),
        }
    }

    fn name(&self) -> String {
        match self {
            OptionAttrs::Implicit(option) => option.name(),
            OptionAttrs::Explicit(option) => option.name(),
        }
    }
}

impl FromField for OptionAttrs {
    fn from_field(field: &syn::Field) -> Result<Self, darling::Error> {
        match ImplicitOptionAttrs::from_field(field) {
            Ok(implicit) => Ok(OptionAttrs::Implicit(implicit)),
            Err(implicit_err) => match ExplicitOptionAttrs::from_field(field) {
                Ok(explicit) => Ok(OptionAttrs::Explicit(explicit)),
                Err(explicit_err) => {
                    Err(darling::Error::multiple(vec![implicit_err, explicit_err]))
                }
            },
        }
    }
}

#[derive(FromField)]
#[darling(attributes(command))]
struct ImplicitOptionAttrs {
    ident: Option<syn::Ident>,
    #[darling(default)]
    name: Option<String>,
    implicit: syn::Path,
}

impl ImplicitOptionAttrs {
    fn name(&self) -> String {
        self.name
            .clone()
            .or_else(|| {
                self.ident
                    .as_ref()
                    .map(|id| id.to_string().to_case(Case::Snake))
            })
            .expect("Missing `name` for option")
    }
}

#[derive(FromField)]
#[darling(attributes(command))]
struct ExplicitOptionAttrs {
    ident: Option<syn::Ident>,
    ty: syn::Type,
    #[darling(default)]
    name: Option<String>,
    description: String,
    #[darling(default)]
    autocomplete: bool,
    #[darling(default)]
    min: Option<Number>,
    #[darling(default)]
    max: Option<Number>,
}

impl ExplicitOptionAttrs {
    fn name(&self) -> String {
        self.name
            .clone()
            .or_else(|| {
                self.ident
                    .as_ref()
                    .map(|id| id.to_string().to_case(Case::Snake))
            })
            .expect("Missing `name` for option")
    }
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
