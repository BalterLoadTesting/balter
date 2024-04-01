use proc_macro::TokenStream;
use proc_macro2::{Span, TokenStream as TokenStream2};
use quote::quote;
use syn::{Ident, ItemFn};

/// Proc macro to denote a Transaction
///
/// NOTE: Currently this macro only works on functions with a `Result<T, E>` return value. This is a
/// restriction which will be lifted soon.
///
/// # Example
/// ```ignore
/// use balter::prelude::*;
///
/// #[transaction]
/// fn my_transaction(arg_1: u32, arg_2: &str) -> Result<String, MyError> {
///     ...
/// }
/// ```
#[proc_macro_attribute]
pub fn transaction(attr: TokenStream, item: TokenStream) -> TokenStream {
    transaction_internal(attr, item).into()
}

fn transaction_internal(_attr: TokenStream, item: TokenStream) -> TokenStream2 {
    let input = syn::parse::<ItemFn>(item).unwrap();

    let ItemFn {
        attrs,
        vis,
        sig,
        block,
    } = input;
    let stmts = &block.stmts;

    let ident = &sig.ident;
    quote! {
        #(#attrs)* #vis #sig {
            ::balter::transaction::transaction_hook(::balter::core::generate_labels!(#ident), async move {
                #(#stmts)*
            }).await
        }
    }
}

/// Proc macro to denote a Scenario
///
/// NOTE: Currently this macro only works on functions which take no arguments and with no return value.
/// (void functions). This is a restriction which will be lifted soon.
///
/// See the `Scenario` struct for more information on the methods this macro provides on functions.
///
/// # Example
/// ```ignore
/// use balter::prelude::*;
///
/// #[scenario]
/// fn my_scenario() {
/// }
/// ```
#[proc_macro_attribute]
pub fn scenario(attr: TokenStream, item: TokenStream) -> TokenStream {
    scenario_internal(attr, item, false).into()
}

/// Proc macro to denote a Scenario
///
/// NOTE: Currently this macro only works on functions which take no arguments and with no return value.
/// (void functions). This is a restriction which will be lifted soon.
///
/// See the `Scenario` struct for more information on the methods this macro provides on functions.
///
/// # Example
/// ```ignore
/// use balter::prelude::*;
///
/// #[scenario]
/// fn my_scenario() {
/// }
/// ```
#[proc_macro_attribute]
pub fn scenario_linkme(attr: TokenStream, item: TokenStream) -> TokenStream {
    scenario_internal(attr, item, true).into()
}

fn scenario_internal(_attr: TokenStream, item: TokenStream, linkme: bool) -> TokenStream2 {
    let input = syn::parse::<ItemFn>(item).expect("Macro only works on fn() items");

    let ItemFn {
        attrs,
        vis,
        sig,
        block,
    } = input;
    let stmts = &block.stmts;

    let new_name = Ident::new(&format!("__balter_{}", sig.ident), Span::call_site());
    let mut new_sig = sig.clone();
    new_sig.ident = new_name.clone();

    let mut scen_sig = sig.clone();
    let scen_name = sig.ident.clone();
    scen_sig.asyncness = None;
    scen_sig.output = syn::parse(
        quote! {
            -> impl ::balter::scenario::ConfigurableScenario<::balter::prelude::RunStatistics>
        }
        .into(),
    )
    .expect("Scenario signature is invalid");

    let res = quote! {
        #(#attrs)* #vis #scen_sig {
            ::balter::scenario::Scenario::new(stringify!(#scen_name), #new_name)
        }

        #(#attrs)* #vis #new_sig {
            #(#stmts)*
        }
    };

    if linkme {
        let mut linkme_sig = sig.clone();
        let linkme_name = Ident::new(&format!("__balter_distr_{}", sig.ident), Span::call_site());
        linkme_sig.ident = linkme_name.clone();
        linkme_sig.asyncness = None;
        linkme_sig.output = syn::parse(
            quote! {
                -> ::core::pin::Pin<Box<dyn ::balter::prelude::DistributedScenario<Output=::balter::prelude::RunStatistics>>>
            }
            .into(),
        )
        .expect("Scenario signature is invalid");

        let static_name = Ident::new(
            &format!("__BALTER_{}", sig.ident.to_string().to_ascii_uppercase()),
            Span::call_site(),
        );

        let mut linkme = quote! {
            #[::balter::runtime::distributed_slice(::balter::runtime::BALTER_SCENARIOS)]
            static #static_name: (&'static str, fn() -> ::core::pin::Pin<Box<dyn ::balter::prelude::DistributedScenario<Output=::balter::prelude::RunStatistics>>>) = (stringify!(#scen_name), #linkme_name);

            // TODO: This definition can almost certainly merge with the #scen_sig definition
            #(#attrs)* #vis #linkme_sig {
                Box::pin(::balter::scenario::Scenario::new(stringify!(#scen_name), #new_name))
            }
        };

        linkme.extend(res);
        linkme
    } else {
        res
    }
}
