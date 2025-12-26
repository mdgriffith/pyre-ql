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
    // Convert snake case to camel case and make singular
    let mut result = String::new();
    let mut capitalize_next = false;
    
    for ch in input.chars() {
        if ch == '_' {
            capitalize_next = true;
        } else if capitalize_next {
            result.push(ch.to_uppercase().next().unwrap_or(ch));
            capitalize_next = false;
        } else {
            result.push(ch);
        }
    }
    
    // Make singular
    singularize(&result)
}

pub fn pluralize(s: &str) -> String {
    // Handle empty string
    if s.is_empty() {
        return s.to_string();
    }
    
    let s_lower = s.to_lowercase();
    let s_len = s.len();
    
    // Check if already plural (ends with -s, -es, -ies, etc.)
    if s_len >= 2 {
        // Check for common plural endings
        if s_lower.ends_with("ies") || s_lower.ends_with("ves") || 
           s_lower.ends_with("es") && !s_lower.ends_with("ses") && !s_lower.ends_with("xes") && 
           !s_lower.ends_with("zes") && !s_lower.ends_with("ches") && !s_lower.ends_with("shes") {
            // Already plural (but not words ending in -ses, -xes, etc. which could be singular like "axis")
            return s.to_string();
        }
        // Check if ends with -s (but not -es, -ies, -ves)
        if s_lower.ends_with('s') && !s_lower.ends_with("es") && !s_lower.ends_with("ies") && 
           !s_lower.ends_with("ves") {
            // Already plural
            return s.to_string();
        }
    }
    
    // Handle common irregular plurals
    let irregular: &[(&str, &str)] = &[
        ("child", "children"),
        ("person", "people"),
        ("man", "men"),
        ("woman", "women"),
        ("mouse", "mice"),
        ("goose", "geese"),
        ("foot", "feet"),
        ("tooth", "teeth"),
        ("ox", "oxen"),
    ];
    
    for (singular, plural) in irregular {
        if s_lower == *singular {
            return preserve_case(s, plural);
        }
    }
    
    // Words ending in -s, -x, -z, -ch, -sh -> add -es
    if s_len >= 2 {
        let last_two = &s_lower[s_len - 2..];
        if last_two == "ch" || last_two == "sh" || s_lower.ends_with('s') || 
           s_lower.ends_with('x') || s_lower.ends_with('z') {
            return format!("{}es", s);
        }
    }
    
    // Words ending in -y preceded by a consonant -> change y to ies
    if s_len >= 2 && s_lower.ends_with('y') {
        let second_last = s_lower.chars().nth(s_len - 2).unwrap();
        if !is_vowel(second_last) {
            return format!("{}ies", &s[..s_len - 1]);
        }
    }
    
    // Words ending in -f or -fe -> change to -ves
    if s_lower.ends_with("fe") && s_len >= 3 {
        return format!("{}ves", &s[..s_len - 2]);
    }
    if s_lower.ends_with('f') && s_len >= 2 {
        let second_last = s_lower.chars().nth(s_len - 2).unwrap();
        if is_vowel(second_last) {
            return format!("{}s", s);
        }
        return format!("{}ves", &s[..s_len - 1]);
    }
    
    // Words ending in -o preceded by a consonant -> add -es
    if s_len >= 2 && s_lower.ends_with('o') {
        let second_last = s_lower.chars().nth(s_len - 2).unwrap();
        if !is_vowel(second_last) {
            return format!("{}es", s);
        }
    }
    
    // Default: add -s
    format!("{}s", s)
}

fn singularize(s: &str) -> String {
    // Handle empty string
    if s.is_empty() {
        return s.to_string();
    }
    
    let s_lower = s.to_lowercase();
    let s_len = s.len();
    
    // Handle common irregular plurals (reverse lookup)
    let irregular: &[(&str, &str)] = &[
        ("children", "child"),
        ("people", "person"),
        ("men", "man"),
        ("women", "woman"),
        ("mice", "mouse"),
        ("geese", "goose"),
        ("feet", "foot"),
        ("teeth", "tooth"),
        ("oxen", "ox"),
    ];
    
    for (plural, singular) in irregular {
        if s_lower == *plural {
            return preserve_case(s, singular);
        }
    }
    
    // Words ending in -ies -> change to -y
    if s_len >= 3 && s_lower.ends_with("ies") {
        let second_last = s_lower.chars().nth(s_len - 4).unwrap_or('a');
        if !is_vowel(second_last) {
            return format!("{}y", &s[..s_len - 3]);
        }
    }
    
    // Words ending in -ves -> change to -f or -fe
    if s_len >= 3 && s_lower.ends_with("ves") {
        // Try -fe first (more common)
        let fe_form = format!("{}fe", &s[..s_len - 3]);
        if s_len >= 4 {
            let second_last = s_lower.chars().nth(s_len - 4).unwrap();
            if is_vowel(second_last) {
                return format!("{}f", &s[..s_len - 3]);
            }
        }
        return fe_form;
    }
    
    // Words ending in -es (but not -ies or -ves)
    if s_len >= 3 && s_lower.ends_with("es") && !s_lower.ends_with("ies") && !s_lower.ends_with("ves") {
        let second_last = s_lower.chars().nth(s_len - 3).unwrap();
        // Check if it's -ch, -sh, -s, -x, -z, or consonant + o
        if s_lower.ends_with("ches") || s_lower.ends_with("shes") || 
           s_lower.ends_with("ses") || s_lower.ends_with("xes") || 
           s_lower.ends_with("zes") || (!is_vowel(second_last) && s_lower.ends_with("oes")) {
            return s[..s_len - 2].to_string();
        }
    }
    
    // Words ending in -s (but not -es, -ies, -ves)
    if s_len >= 2 && s_lower.ends_with('s') && !s_lower.ends_with("es") && 
       !s_lower.ends_with("ies") && !s_lower.ends_with("ves") {
        return s[..s_len - 1].to_string();
    }
    
    // If no plural form detected, return as-is
    s.to_string()
}

fn is_vowel(ch: char) -> bool {
    matches!(ch.to_ascii_lowercase(), 'a' | 'e' | 'i' | 'o' | 'u')
}

fn preserve_case(original: &str, replacement: &str) -> String {
    // Preserve the case pattern of the original word
    if original.chars().next().unwrap().is_uppercase() {
        let mut result = replacement.to_string();
        if let Some(first) = result.chars().next() {
            result.replace_range(..1, &first.to_uppercase().collect::<String>());
        }
        result
    } else {
        replacement.to_string()
    }
}
