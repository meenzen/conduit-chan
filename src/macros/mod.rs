mod admin;
mod cargo;
mod rustc;
mod utils;

use proc_macro::TokenStream;

#[proc_macro_attribute]
pub fn admin_command_dispatch(args: TokenStream, input: TokenStream) -> TokenStream {
	admin::command_dispatch(args, input)
}

#[proc_macro_attribute]
pub fn cargo_manifest(args: TokenStream, input: TokenStream) -> TokenStream { cargo::manifest(args, input) }

#[proc_macro]
pub fn rustc_flags_capture(args: TokenStream) -> TokenStream { rustc::flags_capture(args) }
