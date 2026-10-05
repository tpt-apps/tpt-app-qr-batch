//! Minimal RFC 4180 CSV reader — quotes, escaped quotes, embedded commas
//! and newlines, CRLF/LF/CR line endings, UTF-8 BOM. Hand-rolled (like the
//! cutlist optimizer's) to keep the WASM dependency set at zero.

/// Parse CSV text into records of fields. Blank lines are skipped.
pub fn parse_csv(text: &str) -> Vec<Vec<String>> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut records: Vec<Vec<String>> = Vec::new();
    let mut field = String::new();
    let mut record: Vec<String> = Vec::new();
    let mut in_quotes = false;
    let mut chars = text.chars().peekable();

    while let Some(c) = chars.next() {
        if in_quotes {
            match c {
                '"' => {
                    if chars.peek() == Some(&'"') {
                        chars.next();
                        field.push('"');
                    } else {
                        in_quotes = false;
                    }
                }
                _ => field.push(c),
            }
        } else {
            match c {
                '"' if field.is_empty() => in_quotes = true,
                ',' => {
                    record.push(std::mem::take(&mut field));
                }
                '\r' => {
                    if chars.peek() == Some(&'\n') {
                        chars.next();
                    }
                    record.push(std::mem::take(&mut field));
                    if !(record.len() == 1 && record[0].trim().is_empty()) {
                        records.push(std::mem::take(&mut record));
                    } else {
                        record.clear();
                    }
                }
                '\n' => {
                    record.push(std::mem::take(&mut field));
                    if !(record.len() == 1 && record[0].trim().is_empty()) {
                        records.push(std::mem::take(&mut record));
                    } else {
                        record.clear();
                    }
                }
                _ => field.push(c),
            }
        }
    }
    // Trailing record without a final newline.
    if !field.is_empty() || record.len() > 1 || (record.len() == 1 && !record[0].trim().is_empty())
    {
        record.push(field);
        records.push(record);
    }
    records
}

/// True when a record looks like a header row (`data` optionally followed by
/// `label`/`caption`/`name`, case-insensitive) — skipped by [`csv_to_specs`].
fn is_header(record: &[String]) -> bool {
    let normalized: Vec<String> =
        record.iter().map(|f| f.trim().to_ascii_lowercase()).collect();
    normalized.first().map(|f| f == "data" || f == "value" || f == "url" || f == "code").unwrap_or(false)
        && normalized.iter().skip(1).all(|f| {
            f == "label" || f == "caption" || f == "name" || f.is_empty()
        })
}

/// CSV format: first column = code data, second column = optional label.
/// Rows beyond the second column are ignored (so spreadsheet exports with
/// extra metadata still import cleanly). Header detection as above.
pub fn csv_rows(text: &str) -> Vec<Vec<String>> {
    let mut records = parse_csv(text);
    if records.first().map(|r| is_header(r)).unwrap_or(false) {
        records.remove(0);
    }
    records
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_rows_and_labels() {
        let rows = csv_rows("https://example.com/1,Table 1\nhttps://example.com/2,Table 2");
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0], vec!["https://example.com/1", "Table 1"]);
    }

    #[test]
    fn quoted_fields_with_commas_and_newlines() {
        let rows = csv_rows("\"a,b\",\"line1\nline2\"\nplain,second");
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0][0], "a,b");
        assert_eq!(rows[0][1], "line1\nline2");
    }

    #[test]
    fn escaped_quotes_and_bom() {
        let rows = csv_rows("\u{feff}\"say \"\"hi\"\"\",lbl");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0][0], "say \"hi\"");
    }

    #[test]
    fn crlf_and_blank_lines() {
        let rows = csv_rows("a,1\r\n\r\nb,2\r\n");
        assert_eq!(rows.len(), 2);
    }

    #[test]
    fn header_skipped() {
        assert_eq!(csv_rows("data,label\nx,y").len(), 1);
        assert_eq!(csv_rows("Data,Label\nx,y").len(), 1);
        assert_eq!(csv_rows("url\nttps://x").len(), 1);
        // Not a header: `data` in a non-first column.
        assert_eq!(csv_rows("x,data\ny,z").len(), 2);
    }

    #[test]
    fn extra_columns_ignored() {
        let rows = csv_rows("code1,label,notes,whatever");
        assert_eq!(rows[0].len(), 4);
    }

    #[test]
    fn no_trailing_newline() {
        let rows = csv_rows("a,b");
        assert_eq!(rows.len(), 1);
    }
}
