//! One attribute macro, which returns its input unchanged.

use proc_macro::TokenStream;

/// Returns the annotated item unchanged. `#[tonic::async_trait]` rewrites the
/// item; this one does not, so a difference in the emitted header cannot be
/// blamed on the rewrite.
#[proc_macro_attribute]
pub fn keep(_attribute: TokenStream, item: TokenStream) -> TokenStream {
    item
}
