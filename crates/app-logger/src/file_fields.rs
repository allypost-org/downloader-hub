use tracing_subscriber::{
    field::RecordFields,
    fmt::{
        FormattedFields,
        format::{DefaultFields, FormatFields, JsonFields, Writer},
    },
};

/// Separate [`FormatFields`] type for the file sink so span field formatting is
/// cached independently from the console layer.
///
/// Without this, both `fmt` layers share `FormattedFields<DefaultFields>` in span
/// extensions. Whichever layer creates a span first wins, so a colored console
/// layer can poison file logs with ANSI escapes embedded in span fields.
#[derive(Debug, Default, Clone, Copy)]
pub struct FileFields;

impl<'writer> FormatFields<'writer> for FileFields {
    fn format_fields<R>(&self, writer: Writer<'writer>, fields: R) -> std::fmt::Result
    where
        R: RecordFields,
    {
        DefaultFields::default().format_fields(writer, fields)
    }
}

/// JSON counterpart to [`FileFields`]: isolates span field caches for the file
/// sink while still formatting fields as JSON (required by the json layer).
#[derive(Debug, Default, Clone, Copy)]
pub struct JsonFileFields;

impl<'writer> FormatFields<'writer> for JsonFileFields {
    fn format_fields<R>(&self, writer: Writer<'writer>, fields: R) -> std::fmt::Result
    where
        R: RecordFields,
    {
        JsonFields::default().format_fields(writer, fields)
    }

    fn add_fields(
        &self,
        current: &'writer mut FormattedFields<Self>,
        fields: &tracing::span::Record<'_>,
    ) -> std::fmt::Result {
        let mut json_current =
            FormattedFields::<JsonFields>::new(std::mem::take(&mut current.fields));
        JsonFields::default().add_fields(&mut json_current, fields)?;
        current.fields = json_current.fields;
        Ok(())
    }
}
