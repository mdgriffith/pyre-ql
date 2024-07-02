use inflector::Inflector;
use regex::Regex;

pub fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        None => String::new(),
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
    }
}

pub fn decapitalize(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        None => String::new(),
        Some(f) => f.to_lowercase().collect::<String>() + c.as_str(),
    }
}

pub fn quote(s: &str) -> String {
    format!("\"{}\"", s)
}

pub fn single_quote(s: &str) -> String {
    format!("'{}'", s)
}

pub fn snake_to_camel_and_singular(input: &str) -> String {
    // Convert snake case to camel case
    let re = Regex::new(r"_([a-z])").unwrap();
    let camel_case = re.replace_all(input, |caps: &regex::Captures| caps[1].to_uppercase());

    // Split the string into words (in case there are multiple words)
    let words: Vec<&str> = camel_case.split('_').collect();
    let mut singular_words: Vec<String> = Vec::new();

    // Singularize each word
    for word in words {
        singular_words.push(word.to_string().to_singular());
    }

    // Join the words back together
    singular_words.join("")
}

pub fn pluralize(s: &str) -> String {
    s.to_plural()
}
