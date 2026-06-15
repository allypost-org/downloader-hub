use clap::error::ErrorFormatter;

pub struct MarkdownFormatter;

impl ErrorFormatter for MarkdownFormatter {
    fn format_error(error: &clap::error::Error<Self>) -> clap::builder::StyledStr {
        // Build error message from components - output plain text, let markdown formatter handle styling
        let mut parts = Vec::new();

        // Format the error kind directly (no "Error:" prefix - let markdown formatter add that if needed)
        let kind_str = format!("{:?}", error.kind());
        parts.push(kind_str);

        // Add context information in format "ContextKind: value"
        for (ctx_kind, ctx_value) in error.context() {
            let ctx_str = format!("{:?}: {}", ctx_kind, ctx_value);
            parts.push(ctx_str);
        }

        // Join with newlines - plain text, no markdown yet
        let error_text = parts.join("\n");

        // Convert to markdown - this will detect patterns like "InvalidSubcommand:", "Usage:" etc.
        // and format them properly (bold labels, etc.)
        let markdown = format_error_text_as_markdown(&error_text);

        clap::builder::StyledStr::from(markdown)
    }
}

/// Formats error text by detecting patterns and converting them to markdown
fn format_error_text_as_markdown(text: &str) -> String {
    let mut result = String::new();
    let lines: Vec<&str> = text.lines().collect();
    let mut i = 0;

    while i < lines.len() {
        let line = lines[i].trim();

        // Skip empty lines (we'll add spacing as needed)
        if line.is_empty() {
            i += 1;
            continue;
        }

        // Process all lines through format_bold_patterns to detect and format:
        // - Headers (lines with colons like "Error:", "Usage:")
        // - Lists
        // - Quoted text
        // - Keywords

        // Detect list items (lines starting with common list markers or indented)
        if line.starts_with("  ") || line.starts_with("- ") || line.starts_with("* ") {
            let mut list_items: Vec<String> = Vec::new();
            while i < lines.len() {
                let current_line = lines[i].trim();
                if current_line.is_empty() {
                    break;
                }

                // Check if it's a list item
                if current_line.starts_with("- ") || current_line.starts_with("* ") {
                    let item = current_line
                        .strip_prefix("- ")
                        .or_else(|| current_line.strip_prefix("* "))
                        .unwrap_or(current_line);
                    list_items.push(item.trim().to_string());
                } else if current_line.starts_with("  ") && !list_items.is_empty() {
                    // Continuation of previous item
                    if let Some(last) = list_items.last_mut() {
                        let continuation = current_line.trim();
                        *last = format!("{} {}", last, continuation);
                    }
                } else {
                    break;
                }
                i += 1;
            }

            if !list_items.is_empty() {
                for item in list_items {
                    result.push_str(&format!("- {}\n", item));
                }
                result.push('\n');
                continue;
            }
        }

        // Format the line with bold patterns
        let formatted_line = format_bold_patterns(line);
        result.push_str(&format!("{}\n", formatted_line));
        i += 1;
    }

    result.trim().to_string()
}

/// Formats common patterns that should be bold in markdown
fn format_bold_patterns(line: &str) -> String {
    // Skip if already contains markdown to avoid double-processing
    if line.contains("**") {
        return line.to_string();
    }

    let mut result = String::from(line);

    // Pattern: text in quotes (often argument names) - make them bold
    result = format_quoted_text(&result);

    // Pattern: text before colons (labels like "Error:", "Usage:") - make them bold
    // But only if we haven't already added markdown
    if !result.contains("**") {
        result = format_colon_labels(&result);
    }

    // Pattern: common error keywords that should be bold (only if no markdown yet)
    // Skip this if the line already has a colon label formatted
    if !result.contains("**") {
        result = format_keywords(&result);
    }

    result
}

/// Makes quoted text bold
fn format_quoted_text(text: &str) -> String {
    let mut result = String::new();
    let mut last_pos = 0;
    let mut in_quotes = false;
    let mut quote_start = 0;

    for (pos, ch) in text.char_indices() {
        if ch == '"' {
            if in_quotes {
                // End of quoted section - make it bold
                result.push_str(&text[last_pos..quote_start]); // Text before opening quote
                let quoted_content = &text[quote_start + 1..pos];
                result.push_str(&format!(r#"**"{}"**"#, quoted_content));
                last_pos = pos + 1;
                in_quotes = false;
            } else {
                // Start of quoted section
                result.push_str(&text[last_pos..pos]); // Text before quote
                quote_start = pos;
                in_quotes = true;
            }
        }
    }

    // Add remaining text
    if in_quotes {
        // Unclosed quote - add the rest as-is
        result.push_str(&text[quote_start..]);
    } else {
        result.push_str(&text[last_pos..]);
    }

    result
}

/// Makes text before colons bold (for labels)
fn format_colon_labels(text: &str) -> String {
    // Skip if already contains markdown to avoid double-formatting
    if text.contains("**") {
        return text.to_string();
    }

    // Only format if it looks like a label (single colon, not in quotes, reasonable length)
    if let Some(colon_pos) = text.find(':') {
        // Check if it's not inside quotes
        let before_colon = &text[..colon_pos];
        let after_colon = &text[colon_pos..];

        // Count quotes before the colon to check if we're inside quotes
        let quotes_before = before_colon.matches('"').count();
        if quotes_before.is_multiple_of(2) {
            // Not inside quotes, format as label
            // Only format if it's a reasonable label (not too long, no newlines)
            if before_colon.len() < 50
                && !before_colon.contains('\n')
                && !before_colon.trim().is_empty()
            {
                return format!("**{}**{}", before_colon, after_colon);
            }
        }
    }
    text.to_string()
}

/// Makes common error keywords bold
fn format_keywords(text: &str) -> String {
    // Skip if text already contains markdown formatting to avoid infinite loops
    if text.contains("**") {
        return text.to_string();
    }

    let keywords = [
        "error", "Error", "ERROR", "usage", "Usage", "USAGE", "invalid", "Invalid", "INVALID",
        "required", "Required", "REQUIRED", "missing", "Missing", "MISSING", "unknown", "Unknown",
        "UNKNOWN",
    ];

    let mut result = text.to_string();
    for keyword in &keywords {
        let replacement = format!("**{}**", keyword);
        let mut new_result = String::with_capacity(result.len() + replacement.len());
        let mut last_end = 0;

        // Find all occurrences and replace whole words only
        let mut search_pos = 0;
        while let Some(pos) = result[search_pos..].find(keyword) {
            let actual_pos = search_pos + pos;

            // Check word boundaries safely
            let is_word_start = if actual_pos == 0 {
                true
            } else {
                result[..actual_pos]
                    .chars()
                    .last()
                    .is_none_or(|c| !c.is_alphanumeric())
            };

            let after_start = actual_pos + keyword.len();
            let is_word_end = after_start >= result.len()
                || result[after_start..]
                    .chars()
                    .next()
                    .is_none_or(|c| !c.is_alphanumeric());

            if is_word_start && is_word_end {
                // Add text before the match
                new_result.push_str(&result[last_end..actual_pos]);
                // Add the replacement
                new_result.push_str(&replacement);
                last_end = after_start;
                search_pos = after_start;
            } else {
                // Not a whole word, continue searching after this position
                search_pos = actual_pos + 1;
                if search_pos >= result.len() {
                    break;
                }
            }
        }

        // Add remaining text
        if last_end < result.len() {
            new_result.push_str(&result[last_end..]);
        }

        if !new_result.is_empty() {
            result = new_result;
        }
    }

    result
}
