use crate::{
    network::msdp::MsdpValue,
    state::{AppState, OutputCategory},
};

pub(super) fn format_msdp_value(value: &MsdpValue) -> String {
    match value {
        MsdpValue::String(value) => format!("`{}`", markdown_inline(value)),
        MsdpValue::Array(values) => {
            if values.is_empty() {
                return "`[]`".to_string();
            }
            let values = values
                .iter()
                .map(format_msdp_value)
                .collect::<Vec<_>>()
                .join(", ");
            format!("[{values}]")
        }
        MsdpValue::Table(values) => {
            if values.is_empty() {
                return "`{{}}`".to_string();
            }
            let mut fields = values.iter().collect::<Vec<_>>();
            fields.sort_by_key(|(field, _)| *field);
            let fields = fields
                .into_iter()
                .map(|(key, value)| {
                    format!("`{}`: {}", markdown_inline(key), format_msdp_value(value))
                })
                .collect::<Vec<_>>()
                .join(", ");
            format!("{{ {fields} }}")
        }
    }
}

pub(super) fn markdown_inline(value: &str) -> String {
    value.replace('`', "'").replace('\n', "\\n")
}

pub(super) fn push_output_lines(
    state: &mut AppState,
    message: impl AsRef<str>,
    category: OutputCategory,
) {
    for line in message.as_ref().lines() {
        state.push_output(line.to_string(), category.clone());
    }
}
