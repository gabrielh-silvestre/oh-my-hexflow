pub fn first_line(text: &str) -> &str {
    text.lines().next().unwrap_or_default()
}
