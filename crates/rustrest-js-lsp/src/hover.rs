//! Hover text and signature help.

use lsp_types::{
    Documentation, ParameterInformation, ParameterLabel, SignatureHelp, SignatureInformation,
};

use crate::{
    analysis::{self, Class, Local, Masks, Resolved, is_ident_byte},
    api_model::Member,
};

/// Plain-text hover at byte `offset`: `signature\n\ndescription`. Falls back
/// to the enclosing known call's signature when not on a known identifier.
pub fn hover(text: &str, masks: &Masks, offset: usize, locals: &[Local]) -> Option<String> {
    let offset = offset.min(text.len());
    let class = masks.class_at(offset);
    if class == Class::Comment {
        return None;
    }
    if class == Class::Code
        && let Some((member, qualifier)) = identifier_at(text, masks, offset, locals)
    {
        return Some(format_hover(member, &qualifier));
    }
    let info = analysis::enclosing_call(text, masks, offset, locals)?;
    Some(format_hover(info.member, &info.qualifier))
}

fn format_hover(member: &Member, qualifier: &str) -> String {
    format!("{}\n\n{}", member.signature(qualifier), member.full_doc())
}

/// The API member named by the identifier under the cursor.
fn identifier_at(
    text: &str,
    masks: &Masks,
    offset: usize,
    locals: &[Local],
) -> Option<(&'static Member, String)> {
    let bytes = text.as_bytes();
    let mut end = offset;
    while end < bytes.len() && is_ident_byte(bytes[end]) && masks.class(end) == Class::Code {
        end += 1;
    }
    let chain = analysis::extract_chain(text, masks, end);
    if chain.prefix.is_empty() || chain.prefix_start > offset {
        return None;
    }
    if chain.segs.is_empty() && locals.iter().any(|l| l.name == chain.prefix) {
        return None;
    }
    let Resolved::Type(owner) = analysis::resolve(&chain.segs, locals) else {
        return None;
    };
    let member = owner.member(&chain.prefix)?;
    Some((member, analysis::qualifier(&chain.segs, owner)))
}

/// Signature help for the innermost known call around `offset`.
pub fn signature_help(
    text: &str,
    masks: &Masks,
    offset: usize,
    locals: &[Local],
) -> Option<SignatureHelp> {
    if masks.class_at(offset) == Class::Comment {
        return None;
    }
    let info = analysis::enclosing_call(text, masks, offset, locals)?;
    let member = info.member;
    let parameters = member
        .params
        .unwrap_or(&[])
        .iter()
        .map(|p| ParameterInformation {
            label: ParameterLabel::Simple(format!("{}: {}", p.name, p.ty)),
            documentation: None,
        })
        .collect();
    Some(SignatureHelp {
        signatures: vec![SignatureInformation {
            label: member.signature(&info.qualifier),
            documentation: Some(Documentation::String(member.full_doc())),
            parameters: Some(parameters),
            active_parameter: Some(info.active_param),
        }],
        active_signature: Some(0),
        active_parameter: Some(info.active_param),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hover_at(src: &str, marker: &str) -> Option<String> {
        let offset = src.find(marker).unwrap();
        let src = src.replace(marker, "");
        let masks = Masks::new(&src);
        let locals = analysis::scan_locals(&src, &masks);
        hover(&src, &masks, offset, &locals)
    }

    #[test]
    fn hover_on_member() {
        let h = hover_at("pm.environment.s|et(", "|").unwrap();
        assert_eq!(
            h,
            "pm.environment.set(key: string, value: string): void\n\nSets `key` to `value` (stored as a string)."
        );
        let h = hover_at("|pm.test('a', f)", "|").unwrap();
        assert!(h.starts_with("pm: Pm\n\n"));
        let h = hover_at("pm.expect(1).to.eq|ual(2)", "|").unwrap();
        assert!(h.starts_with("Assertion.equal(expected: any): void"));
        let h = hover_at("pm.te|st('a', f)", "|").unwrap();
        assert!(h.contains("post-response scripts only"));
    }

    #[test]
    fn hover_inside_call_falls_back_to_signature() {
        let h = hover_at("pm.environment.set(\"k|ey\", value)", "|").unwrap();
        assert!(h.starts_with("pm.environment.set(key: string, value: string): void"));
        let h = hover_at("pm.environment.set(\"key\", va|lue)", "|").unwrap();
        assert!(h.starts_with("pm.environment.set("));
        assert!(hover_at("foo(ba|r)", "|").is_none());
        assert!(hover_at("// pm.environment.s|et(", "|").is_none());
        let local = hover_at("const x = 1; x|;", "|");
        assert!(local.is_none());
    }

    #[test]
    fn signature_help_active_param() {
        let src = "pm.environment.set('k', ";
        let masks = Masks::new(src);
        let sh = signature_help(src, &masks, src.len(), &[]).unwrap();
        assert_eq!(sh.active_parameter, Some(1));
        assert_eq!(
            sh.signatures[0].label,
            "pm.environment.set(key: string, value: string): void"
        );
        let src = "console.log(a, b, c, ";
        let masks = Masks::new(src);
        let sh = signature_help(src, &masks, src.len(), &[]).unwrap();
        assert_eq!(sh.active_parameter, Some(0));
    }
}
