// src/utils/xml_editor.rs

/// Cleans the HTML content by removing specified tags and their content.
/// This is a simplified, custom-built HTML stripper. It does not build a full DOM tree.
/// It works by iterating through the HTML string and skipping blocks enclosed by
/// opening and closing tags specified in `unwanted_tags`.
///
/// # Arguments
/// * `html_content` - The raw HTML string to clean.
/// * `unwanted_tags` - A slice of tag names (e.g., "script", "style") to remove.
///
/// # Returns
/// A `String` containing the cleaned HTML content.
pub fn clean_html_string(html_content: &str, unwanted_tags: &[&str]) -> String {
    let mut cleaned_html = String::with_capacity(html_content.len());
    let mut chars = html_content.chars().peekable();
    let mut in_unwanted_tag = false;
    let mut current_tag_name = String::new();

    while let Some(c) = chars.next() {
        if c == '<' {
            // Potential start of a tag
            let mut temp_chars = chars.clone(); // Peek without consuming
            let mut tag_name_buf = String::new();
            
            // Check for closing tag (e.g., </script>)
            if let Some('/') = temp_chars.peek().copied() {
                temp_chars.next(); // Consume '/'
                while let Some(&tag_char) = temp_chars.peek() {
                    if tag_char.is_alphanumeric() || tag_char == '-' {
                        tag_name_buf.push(temp_chars.next().unwrap());
                    } else {
                        break;
                    }
                }
                if tag_name_buf == current_tag_name && in_unwanted_tag {
                    // Found closing tag for the current unwanted block
                    in_unwanted_tag = false;
                    current_tag_name.clear();
                    // Consume characters until '>'
                    while let Some(gt_char) = chars.next() {
                        if gt_char == '>' {
                            break;
                        }
                    }
                    continue; // Skip the closing tag
                }
            }

            // Check for opening tag (e.g., <script>)
            tag_name_buf.clear();
            let mut initial_temp_chars = chars.clone(); // Reset for opening tag check
            while let Some(&tag_char) = initial_temp_chars.peek() {
                if tag_char.is_alphanumeric() || tag_char == '-' {
                    tag_name_buf.push(initial_temp_chars.next().unwrap());
                } else {
                    break;
                }
            }

            if unwanted_tags.contains(&tag_name_buf.as_str()) {
                in_unwanted_tag = true;
                current_tag_name = tag_name_buf;
                // Consume characters until '>' for the opening tag
                while let Some(gt_char) = chars.next() {
                    if gt_char == '>' {
                        break;
                    }
                }
                continue; // Skip the opening tag
            } else {
                // Not an unwanted tag, append '<' and then process the rest
                cleaned_html.push('<');
                cleaned_html.push_str(&tag_name_buf);
                chars = initial_temp_chars; // Restore chars to where initial_temp_chars left off
                // Continue to append characters until '>' (for non-unwanted tags)
                while let Some(&inner_char) = chars.peek() {
                    if inner_char == '>' {
                        cleaned_html.push(chars.next().unwrap()); // Consume and append '>'
                        break;
                    } else {
                        cleaned_html.push(chars.next().unwrap());
                    }
                }
                continue; // Move to next char in main loop
            }
        }
        
        if in_unwanted_tag {
            // Skip characters within an unwanted tag block
            continue;
        } else {
            cleaned_html.push(c);
        }
    }
    cleaned_html
}
