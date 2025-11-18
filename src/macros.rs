/// Convenience macro for writing quick shell-style scripts.
#[macro_export]
macro_rules! qshr {
    ($($body:tt)*) => {{
        use $crate::prelude::*;
        let __qshr_entry = || -> $crate::Result<()> {
            $crate::__qshr_execute! { $($body)* }
        };
        __qshr_entry()
    }};
}

#[doc(hidden)]
#[macro_export]
macro_rules! __qshr_build_pipeline {
    ($cmd:literal) => {
        $crate::macros::literal_command($cmd)
    };
    ($cmd:literal | $($rest:tt)+) => {{
        $crate::macros::literal_command($cmd).pipe($crate::__qshr_build_pipeline!($($rest)+))
    }};
}

#[doc(hidden)]
#[allow(redundant_semicolons)]
#[macro_export]
macro_rules! __qshr_execute {
    () => {
        Ok(())
    };
    (env $key:literal = $value:expr ; $($rest:tt)*) => {{
        $crate::set_var($key, $value);
        $crate::__qshr_execute! { $($rest)* }
    }};
    (env $key:literal = $value:expr) => {{
        $crate::set_var($key, $value);
        Ok(())
    }};
    (unset $key:literal ; $($rest:tt)*) => {{
        $crate::remove_var($key);
        $crate::__qshr_execute! { $($rest)* }
    }};
    (unset $key:literal) => {{
        $crate::remove_var($key);
        Ok(())
    }};
    ($first:literal $(| $next:literal)+ ; $($rest:tt)*) => {{
        $crate::__qshr_build_pipeline!($first $(| $next)+).run()?;
        $crate::__qshr_execute! { $($rest)* }
    }};
    ($first:literal $(| $next:literal)+) => {{
        $crate::__qshr_build_pipeline!($first $(| $next)+).run()?;
        Ok(())
    }};
    ($cmd:literal ; $($rest:tt)*) => {{
        $crate::macros::literal_command($cmd).run()?;
        $crate::__qshr_execute! { $($rest)* }
    }};
    ($cmd:literal) => {{
        $crate::macros::literal_command($cmd).run()?;
        Ok(())
    }};
    ($stmt:stmt ; $($rest:tt)*) => {{
        $stmt;
        $crate::__qshr_execute! { $($rest)* }
    }};
    ($stmt:stmt) => {{
        $stmt;
        Ok(())
    }};
    ($expr:expr) => {{
        $expr
    }};
}

#[doc(hidden)]
pub fn interpolate_command(template: &str) -> String {
    let mut out = String::with_capacity(template.len());
    let mut chars = template.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '$' {
            match chars.peek() {
                Some('$') => {
                    out.push('$');
                    chars.next();
                }
                Some('{') => {
                    chars.next();
                    let mut name = String::new();
                    while let Some(&c) = chars.peek() {
                        chars.next();
                        if c == '}' {
                            break;
                        }
                        name.push(c);
                    }
                    out.push_str(&resolve_var(&name));
                }
                Some(&c) if is_ident_start(c) => {
                    let mut name = String::new();
                    name.push(c);
                    chars.next();
                    while let Some(&c) = chars.peek() {
                        if is_ident_continue(c) {
                            name.push(c);
                            chars.next();
                        } else {
                            break;
                        }
                    }
                    out.push_str(&resolve_var(&name));
                }
                _ => out.push(ch),
            }
        } else {
            out.push(ch);
        }
    }
    out
}

#[doc(hidden)]
pub fn literal_command(template: &str) -> crate::Command {
    crate::sh(interpolate_command(template))
}

fn resolve_var(name: &str) -> String {
    match crate::var(name) {
        Some(val) => val.to_string_lossy().into_owned(),
        None => String::new(),
    }
}

fn is_ident_start(c: char) -> bool {
    c == '_' || c.is_ascii_alphabetic()
}

fn is_ident_continue(c: char) -> bool {
    c == '_' || c.is_ascii_alphanumeric()
}

#[cfg(test)]
mod tests {
    use super::interpolate_command;
    use crate::{remove_var, set_var};

    #[test]
    fn interpolates_env_vars() {
        set_var("QSHR_MACRO_TEST", "value");
        let interpolated = interpolate_command("echo $QSHR_MACRO_TEST ${QSHR_MACRO_TEST} $$");
        assert_eq!(interpolated, "echo value value $");
        remove_var("QSHR_MACRO_TEST");
    }
}
