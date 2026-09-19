//! The two string rules this crate owns — plan-04 §7.
//!
//! Both are pure functions over `&str`, which is what lets every case in
//! plan-04 §12 be a table rather than a fixture.

/// tonic's method-name rule: the RPC name as the generated trait spells it.
///
/// # This is measured against the generator, not reasoned about
///
/// Plan-04 §7 forbids guessing, and the acronym cases are where a reasonable
/// guess goes wrong. MEASURED 2026-09-19 by compiling a `.proto` with
/// `tonic-prost-build` 0.14.6 and reading the generated trait:
///
/// ```text
/// GetWidgetByID  -> get_widget_by_id
/// ExportCSV      -> export_csv
/// HTTPProxy      -> http_proxy
/// GetHTTP2Stream -> get_http2_stream
/// V2Migrate      -> v2_migrate
/// ```
///
/// The generator's output for that file is checked in at
/// `tests/fixtures/oracle/acme.api.v1.methods.txt`, and
/// `snake_case_matches_generated_trait` asserts this function against it. The
/// rule implemented here is the one those cases pin: a `_` goes before an
/// uppercase letter when the previous character is lowercase or a digit, or
/// when the previous character is uppercase and the next one is lowercase.
pub fn camel_to_snake(rpc: &str) -> String {
    let chars: Vec<char> = rpc.chars().collect();
    let mut out = String::with_capacity(rpc.len() + 4);
    for (index, current) in chars.iter().copied().enumerate() {
        if current.is_uppercase() && index > 0 {
            let previous = chars[index - 1];
            let next_is_lower = chars.get(index + 1).is_some_and(|next| next.is_lowercase());
            let boundary = previous.is_lowercase()
                || previous.is_ascii_digit()
                || (previous.is_uppercase() && next_is_lower);
            if boundary && !out.ends_with('_') {
                out.push('_');
            }
        }
        out.extend(current.to_lowercase());
    }
    out
}

/// Does this impl header implement `service` as a trait?
///
/// # The grammar, and why it is parsed anchored
///
/// Plan-03 §8 renders an impl `Symbol`'s `raw_kind` as
///
/// ```text
/// impl <Trait> for <SelfTy>      // trait impl
/// impl <SelfTy>                  // inherent impl
/// ```
///
/// with `<Trait>` the trait's **declared** name rather than the path it was
/// imported by. MEASURED (plan-04 §1 M3) that the declared name equals the
/// proto service name, which is what lets this crate compare the two directly
/// and never resolve a Rust import.
///
/// A substring search for the service name would match `impl WidgetsExt for …`
/// and `impl Foo for WidgetsClient<T>`. Both are real shapes, so the parse
/// requires the literal `impl ` prefix, splits on the ` for ` keyword, strips
/// generic arguments, takes the segment after the last `::`, and compares
/// exactly.
pub fn impl_header_names_trait(raw_kind: &str, service: &str) -> bool {
    trait_of_impl_header(raw_kind).is_some_and(|name| name == service)
}

/// Does this impl header implement an inherent impl on `self_ty`?
///
/// Plan-04 §11's consumed-side binding needs the other half of the grammar:
/// the generated client method sits in `impl <Service>Client<T>`, an **inherent**
/// impl, so a header with a trait half is the wrong shape however it is named.
pub fn impl_header_names_self_type(raw_kind: &str, self_ty: &str) -> bool {
    let Some(body) = raw_kind.strip_prefix("impl ") else {
        return false;
    };
    if split_on_for(body).is_some() {
        return false;
    }
    last_segment(strip_generics(body)) == self_ty
}

/// The trait half of an impl header, or `None` when there is not one.
fn trait_of_impl_header(raw_kind: &str) -> Option<&str> {
    let body = raw_kind.strip_prefix("impl ")?;
    let (trait_half, _self_ty) = split_on_for(body)?;
    Some(last_segment(strip_generics(trait_half)))
}

/// Split an impl header's body on the `for` **keyword**.
///
/// Whitespace on both sides is required, so `impl Reformat` stays an inherent
/// impl rather than splitting inside a name.
fn split_on_for(body: &str) -> Option<(&str, &str)> {
    let mut search = 0usize;
    while let Some(offset) = body[search..].find(" for ") {
        let at = search + offset;
        let (left, right) = (&body[..at], &body[at + " for ".len()..]);
        if !left.is_empty() && !right.is_empty() {
            return Some((left.trim(), right.trim()));
        }
        search = at + 1;
    }
    None
}

/// `Widgets<Request>` → `Widgets`.
fn strip_generics(name: &str) -> &str {
    match name.find('<') {
        Some(at) => name[..at].trim_end(),
        None => name.trim_end(),
    }
}

/// `pb::v1::Widgets` → `Widgets`.
fn last_segment(name: &str) -> &str {
    name.rsplit("::").next().unwrap_or(name).trim()
}
