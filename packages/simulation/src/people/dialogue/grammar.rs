//! The line template grammar.
//!
//! - `{var}` inserts a variable; `{Var}` capitalises it.
//! - `{a var}` / `{A var}` insert it with "a"/"an" in front.
//! - `[one|two|three]` picks one alternative (alternatives may hold variables, not brackets).
//!
//! The first letter of a line is capitalised and doubled spaces collapse, so variables can be
//! dropped into the start of a sentence freely.

use std::collections::BTreeMap;

use super::super::PeopleRng;

/// Variables available to a template.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Vars(BTreeMap<&'static str, String>);

impl Vars {
    #[must_use]
    pub const fn new() -> Self {
        Self(BTreeMap::new())
    }

    pub fn set(&mut self, key: &'static str, value: impl Into<String>) {
        self.0.insert(key, value.into());
    }

    #[must_use]
    pub fn get(&self, key: &str) -> Option<&str> {
        self.0.get(key).map(String::as_str)
    }

    #[must_use]
    pub fn has(&self, key: &str) -> bool {
        self.0.contains_key(key)
    }
}

fn capitalise(text: &str) -> String {
    let mut chars = text.chars();
    chars.next().map_or_else(String::new, |first| {
        first.to_uppercase().chain(chars).collect()
    })
}

fn article(word: &str) -> &'static str {
    let lower = word.to_lowercase();
    let vowel_sound = lower.starts_with(['a', 'e', 'i', 'o'])
        || (lower.starts_with('u') && !lower.starts_with("uni") && !lower.starts_with("use"))
        || lower.starts_with("hour")
        || lower.starts_with("honest");
    if vowel_sound { "an" } else { "a" }
}

/// One `{...}` reference: (variable, capitalise, article).
fn parse_ref(inner: &str) -> (&str, bool, Option<bool>) {
    let (article, name) = match (inner.strip_prefix("a "), inner.strip_prefix("A ")) {
        (Some(rest), _) => (Some(false), rest),
        (None, Some(rest)) => (Some(true), rest),
        (None, None) => (None, inner),
    };
    let capital = name.chars().next().is_some_and(char::is_uppercase);
    (name, capital, article)
}

/// The variable names a template uses (lower-cased), or an error for broken syntax.
///
/// # Errors
///
/// Unbalanced or nested brackets and braces, or an empty alternative.
pub fn variables(template: &str) -> Result<Vec<String>, String> {
    let mut found = Vec::new();
    let mut in_brace: Option<usize> = None;
    let mut in_bracket = false;
    for (index, c) in template.char_indices() {
        match c {
            '{' if in_brace.is_none() => in_brace = Some(index + 1),
            '}' => {
                let start = in_brace.take().ok_or("stray '}'")?;
                let (name, _, _) = parse_ref(&template[start..index]);
                if name.is_empty() {
                    return Err("empty variable".into());
                }
                found.push(name.to_lowercase());
            }
            '[' if !in_bracket && in_brace.is_none() => in_bracket = true,
            ']' if in_bracket => in_bracket = false,
            '|' if !in_bracket => return Err("'|' outside brackets".into()),
            '{' | '[' | ']' => return Err(format!("unexpected '{c}'")),
            _ => {}
        }
    }
    if in_brace.is_some() || in_bracket {
        return Err("unclosed bracket".into());
    }
    if template.contains("[|") || template.contains("||") || template.contains("|]") {
        return Err("empty alternative".into());
    }
    Ok(found)
}

/// Checks a template's syntax and that it only uses `allowed` variables.
///
/// # Errors
///
/// A message naming the problem.
pub fn check(template: &str, allowed: &[&str]) -> Result<(), String> {
    for name in variables(template)? {
        if !allowed.contains(&name.as_str()) {
            return Err(format!("unknown variable {{{name}}}"));
        }
    }
    Ok(())
}

/// Expands `template` with `vars`, rolling alternatives with `rng`. A missing variable expands
/// to "someone" (the content lint keeps that from happening in practice).
pub fn expand(template: &str, vars: &Vars, rng: &mut PeopleRng) -> String {
    // Alternatives first, so a picked alternative can still hold variables.
    let mut picked = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(open) = rest.find('[') {
        picked.push_str(&rest[..open]);
        let Some(close) = rest[open..].find(']') else {
            break;
        };
        let options: Vec<&str> = rest[open + 1..open + close].split('|').collect();
        picked.push_str(rng.pick(&options));
        rest = &rest[open + close + 1..];
    }
    picked.push_str(rest);

    let mut out = String::with_capacity(picked.len() + 16);
    let mut rest = picked.as_str();
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let Some(close) = rest[open..].find('}') else {
            break;
        };
        let (name, capital, with_article) = parse_ref(&rest[open + 1..open + close]);
        let value = vars.get(&name.to_lowercase()).unwrap_or("???").to_owned();
        let value = match with_article {
            Some(capital_article) => {
                let word = article(&value);
                let word = if capital_article {
                    capitalise(word)
                } else {
                    word.to_owned()
                };
                format!("{word} {value}")
            }
            None if capital => capitalise(&value),
            None => value,
        };
        out.push_str(&value);
        rest = &rest[open + close + 1..];
    }
    out.push_str(rest);
    while out.contains("  ") {
        out = out.replace("  ", " ");
    }
    capitalise(out.trim())
}

#[cfg(test)]
mod tests {
    use super::{PeopleRng, Vars, check, expand, variables};

    #[test]
    fn expands_variables_articles_and_alternatives() {
        let mut vars = Vars::new();
        vars.set("place", "old derelict");
        vars.set("name", "wren");
        let mut rng = PeopleRng::new(1, 2, 3, 4);
        assert_eq!(
            expand("{Name} found {a place}.", &vars, &mut rng),
            "Wren found an old derelict."
        );
        assert_eq!(expand("{A place}!", &vars, &mut rng), "An old derelict!");
        let mut seen = std::collections::BTreeSet::new();
        for _ in 0..40 {
            seen.insert(expand("[hello|howdy] {name}", &vars, &mut rng));
        }
        assert_eq!(
            seen.into_iter().collect::<Vec<_>>(),
            vec!["Hello wren".to_owned(), "Howdy wren".to_owned()]
        );
    }

    #[test]
    fn checks_syntax_and_variables() {
        assert_eq!(
            variables("{Name} and {a place} [x|{band}]").unwrap(),
            vec!["name", "place", "band"]
        );
        assert!(check("{name}", &["name"]).is_ok());
        assert!(check("{nope}", &["name"]).is_err());
        assert!(check("[a|b", &[]).is_err());
        assert!(check("[a||b]", &[]).is_err());
        assert!(check("a|b", &[]).is_err());
        assert!(check("{name", &["name"]).is_err());
    }
}
