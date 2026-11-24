//! Template rendering with `minijinja`, compatible with the Jinja2 use of the reference
//! (plan §3.9): `StrictUndefined`, no trailing newline kept, no auto-escaping, and values
//! formatted the way Python's `str()` does (`None`, `True`, `1e+20`, …).

use minijinja::value::ValueKind;
use minijinja::{
    AutoEscape, Environment, Error, Output, State, UndefinedBehavior, Value as JinjaValue,
};
use serde_json::Value;

/// Template compile or render failures.
#[derive(Debug, thiserror::Error)]
pub enum TemplateError {
    /// The template does not compile.
    #[error("template `{name}` has a syntax error: {source}")]
    Syntax {
        /// Template name.
        name: String,
        /// The `minijinja` error.
        #[source]
        source: Box<Error>,
    },
    /// Rendering failed, for example on an undefined variable.
    #[error("rendering template `{name}` failed: {source}")]
    Render {
        /// Template name.
        name: String,
        /// The `minijinja` error.
        #[source]
        source: Box<Error>,
    },
    /// No template has this name.
    #[error("unknown template `{0}`")]
    Unknown(String),
}

/// A set of templates compiled once and rendered many times.
pub struct Templates {
    env: Environment<'static>,
}

impl Default for Templates {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for Templates {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Templates").finish_non_exhaustive()
    }
}

impl Templates {
    /// An empty set with Jinja2-compatible settings.
    pub fn new() -> Self {
        let mut env = Environment::new();
        env.set_undefined_behavior(UndefinedBehavior::Strict);
        // Jinja2's default `keep_trailing_newline=False` is also minijinja's default.
        env.set_auto_escape_callback(|_| AutoEscape::None);
        env.set_formatter(python_formatter);
        Self { env }
    }

    /// Compiles and stores a template.
    pub fn add(
        &mut self,
        name: impl Into<String>,
        source: impl Into<String>,
    ) -> Result<(), TemplateError> {
        let name = name.into();
        self.env
            .add_template_owned(name.clone(), source.into())
            .map_err(|e| TemplateError::Syntax {
                name,
                source: Box::new(e),
            })
    }

    /// Renders a stored template with a JSON context.
    pub fn render(&self, name: &str, context: &Value) -> Result<String, TemplateError> {
        let template = self
            .env
            .get_template(name)
            .map_err(|_| TemplateError::Unknown(name.to_owned()))?;
        template
            .render(JinjaValue::from(minijinja::value::Serde(context)))
            .map_err(|e| TemplateError::Render {
                name: name.to_owned(),
                source: Box::new(e),
            })
    }
}

/// Compiles and renders a template once (Python's `Template(source).render(**context)`).
pub fn render_once(source: &str, context: &Value) -> Result<String, TemplateError> {
    let mut templates = Templates::new();
    templates.add("<inline>", source)?;
    templates.render("<inline>", context)
}

/// Writes values like Python's `str()`; everything else uses minijinja's default formatting.
fn python_formatter(out: &mut Output, state: &mut State, value: &JinjaValue) -> Result<(), Error> {
    match value.kind() {
        ValueKind::None => out.write_str("None").map_err(Error::from),
        ValueKind::Bool => out
            .write_str(if value.is_true() { "True" } else { "False" })
            .map_err(Error::from),
        ValueKind::Number if !value.is_integer() => match f64::try_from(value.clone()) {
            Ok(number) => out
                .write_str(&python_float_repr(number))
                .map_err(Error::from),
            Err(_) => minijinja::escape_formatter(out, state, value),
        },
        _ => minijinja::escape_formatter(out, state, value),
    }
}

/// Python's `repr(float)`: the shortest round-trip digits, fixed notation for exponents in
/// `-4..16`, scientific notation (`1e+20`, `1e-05`) otherwise.
pub fn python_float_repr(number: f64) -> String {
    if number.is_nan() {
        return "nan".to_owned();
    }
    if number.is_infinite() {
        return if number > 0.0 { "inf" } else { "-inf" }.to_owned();
    }
    // Rust's `{:e}` gives the shortest round-trip digits, for example `-1.2345e-5`.
    let scientific = format!("{number:e}");
    let (mantissa, exponent) = scientific
        .split_once('e')
        .unwrap_or((scientific.as_str(), "0"));
    let exponent: i32 = exponent.parse().unwrap_or(0);
    let (sign, mantissa) = match mantissa.strip_prefix('-') {
        Some(rest) => ("-", rest),
        None => ("", mantissa),
    };
    let digits: String = mantissa.chars().filter(char::is_ascii_digit).collect();
    if (-4..16).contains(&exponent) {
        if exponent < 0 {
            let zeros = "0".repeat(usize::try_from(-exponent - 1).unwrap_or(0));
            return format!("{sign}0.{zeros}{digits}");
        }
        let point = usize::try_from(exponent).unwrap_or(0) + 1;
        return match digits.split_at_checked(point) {
            Some((int, frac)) if !frac.is_empty() => format!("{sign}{int}.{frac}"),
            _ => format!(
                "{sign}{digits}{}.0",
                "0".repeat(point.saturating_sub(digits.len()))
            ),
        };
    }
    let mantissa = match digits.split_at_checked(1) {
        Some((first, rest)) if !rest.is_empty() => format!("{first}.{rest}"),
        _ => digits.clone(),
    };
    let exp_sign = if exponent < 0 { '-' } else { '+' };
    format!("{sign}{mantissa}e{exp_sign}{:02}", exponent.unsigned_abs())
}

#[cfg(test)]
mod tests {
    use super::python_float_repr;

    #[test]
    fn float_repr_matches_python() {
        for (value, expected) in [
            (1.0, "1.0"),
            (0.1, "0.1"),
            (1e20, "1e+20"),
            (1e-5, "1e-05"),
            (123_456.789, "123456.789"),
            (-0.0, "-0.0"),
            (1e16, "1e+16"),
            (1e15, "1000000000000000.0"),
            (2.5e-4, "0.00025"),
            (1.5e-300, "1.5e-300"),
            (-2.5, "-2.5"),
            (100.0, "100.0"),
            (0.30000000000000004, "0.30000000000000004"),
            (f64::INFINITY, "inf"),
            (f64::NAN, "nan"),
        ] {
            assert_eq!(python_float_repr(value), expected, "{value}");
        }
    }
}
