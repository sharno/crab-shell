/// Convenience macro for writing quick shell-style scripts.
#[macro_export]
macro_rules! qshr {
    ($($body:tt)*) => {{
        #[allow(unused_imports)]
        use $crate::prelude::*;
        let __qshr_entry = || -> $crate::Result<()> {
            $crate::__qshr_execute! { $($body)* }
        };
        __qshr_entry()
    }};
}

/// Macro to build a [`Command`](crate::Command) in place.
#[macro_export]
macro_rules! cmd {
    ($program:expr $(, $arg:expr )* $(,)?) => {{
        let mut __cmd = $crate::Command::new($program);
        $(
            __cmd = __cmd.arg($arg);
        )*
        __cmd
    }};
}

/// Macro to compose a [`Pipeline`](crate::Pipeline) from commands or string literals.
#[macro_export]
macro_rules! pipeline {
    ($($body:tt)+) => {{
        $crate::__qshr_build_expr_pipeline!($($body)+)
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
#[macro_export]
macro_rules! __qshr_build_expr_pipeline {
    ($($tokens:tt)+) => {
        $crate::__qshr_parse_expr_pipeline!(() $($tokens)+)
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __qshr_parse_expr_pipeline {
    (($($current:tt)*) | $($rest:tt)+) => {{
        $crate::__qshr_expr_stage!($($current)*).pipe($crate::__qshr_parse_expr_pipeline!(() $($rest)+))
    }};
    (($($current:tt)*)) => {
        $crate::__qshr_expr_stage!($($current)*)
    };
    (($($current:tt)*) $token:tt $($rest:tt)*) => {
        $crate::__qshr_parse_expr_pipeline!(($($current)* $token) $($rest)*)
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __qshr_expr_stage {
    ($cmd:literal) => {
        $crate::macros::literal_command($cmd)
    };
    ($($expr:tt)+) => {{
        $($expr)+
    }};
}

#[doc(hidden)]
#[allow(redundant_semicolons)]
#[macro_export]
macro_rules! __qshr_execute {
    () => {
        Ok(())
    };
    (cd($path:expr) { $($block:tt)* } ; $($rest:tt)*) => {{
        $crate::macros::with_dir($path, || $crate::__qshr_execute! { $($block)* })?;
        $crate::__qshr_execute! { $($rest)* }
    }};
    (cd($path:expr) { $($block:tt)* }) => {{
        $crate::macros::with_dir($path, || $crate::__qshr_execute! { $($block)* })
    }};
    (parallel { $($block:tt)* } $({ $($more:tt)* })+ ; $($rest:tt)*) => {{
        $crate::__qshr_parallel_blocks!({ $($block)* } $({ $($more)* })+)?;
        $crate::__qshr_execute! { $($rest)* }
    }};
    (parallel { $($block:tt)* } $({ $($more:tt)* })+) => {{
        $crate::__qshr_parallel_blocks!({ $($block)* } $({ $($more)* })+)
    }};
    (env $key:literal = $value:expr ; $($rest:tt)*) => {{
        $crate::set_var($key, $value);
        $crate::__qshr_execute! { $($rest)* }
    }};
    (env $key:literal = $value:expr) => {{
        $crate::set_var($key, $value);
        Ok(())
    }};
    (run $cmd:expr ; $($rest:tt)*) => {{
        $crate::macros::run_commandlike($cmd)?;
        $crate::__qshr_execute! { $($rest)* }
    }};
    (run $cmd:expr) => {{
        $crate::macros::run_commandlike($cmd)
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
#[macro_export]
macro_rules! __qshr_parallel_blocks {
    ({ $($block:tt)* } $({ $($rest:tt)* })+ ) => {{
        let mut handles: ::std::vec::Vec<::std::thread::JoinHandle<$crate::Result<()>>> = ::std::vec::Vec::new();
        $crate::__qshr_spawn_parallel!(handles, { $($block)* } $({ $($rest)* })+);
        for handle in handles {
            handle.join().expect("parallel block panicked")?;
        }
        Ok::<(), $crate::Error>(())
    }};
}

#[doc(hidden)]
#[macro_export]
macro_rules! __qshr_spawn_parallel {
    ($handles:ident, ) => {};
    ($handles:ident, { $($block:tt)* } $($rest:tt)*) => {{
        $handles.push(::std::thread::spawn(move || $crate::__qshr_execute! { $($block)* }));
        $crate::__qshr_spawn_parallel!($handles, $($rest)*);
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

pub trait MacroRunnable {
    fn run_from_macro(self) -> crate::Result<()>;
}

impl MacroRunnable for crate::Command {
    fn run_from_macro(self) -> crate::Result<()> {
        self.run()
    }
}

impl MacroRunnable for &crate::Command {
    fn run_from_macro(self) -> crate::Result<()> {
        self.run()
    }
}

impl MacroRunnable for crate::Pipeline {
    fn run_from_macro(self) -> crate::Result<()> {
        self.run()
    }
}

impl MacroRunnable for &crate::Pipeline {
    fn run_from_macro(self) -> crate::Result<()> {
        self.run()
    }
}

pub fn run_commandlike(cmd: impl MacroRunnable) -> crate::Result<()> {
    cmd.run_from_macro()
}

pub fn with_dir(
    path: impl AsRef<std::path::Path>,
    f: impl FnOnce() -> crate::Result<()>,
) -> crate::Result<()> {
    use std::env;
    let original = env::current_dir()?;
    env::set_current_dir(path)?;
    let result = f();
    env::set_current_dir(original)?;
    result
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
    use super::{interpolate_command, literal_command, with_dir};
    use crate::{remove_var, set_var, sh};
    use std::env;

    #[test]
    fn interpolates_env_vars() {
        set_var("QSHR_MACRO_TEST", "value");
        let interpolated = interpolate_command("echo $QSHR_MACRO_TEST ${QSHR_MACRO_TEST} $$");
        assert_eq!(interpolated, "echo value value $");
        remove_var("QSHR_MACRO_TEST");
    }

    #[test]
    fn with_dir_restores() -> crate::Result<()> {
        let original = env::current_dir()?;
        let temp = tempfile::tempdir()?;
        with_dir(temp.path(), || {
            let now = env::current_dir()?;
            assert_eq!(now, temp.path());
            Ok(())
        })?;
        assert_eq!(env::current_dir()?, original);
        Ok(())
    }

    #[test]
    fn literal_command_executes() -> crate::Result<()> {
        let output = literal_command("echo literal-test").stdout_text()?;
        assert!(output.contains("literal-test"));
        Ok(())
    }

    #[test]
    fn pipeline_macro_builds_mixed_stages() -> crate::Result<()> {
        let pipe = crate::pipeline!(sh("echo expr-stage") | "more");
        let output = pipe.stdout_text()?;
        assert!(output.contains("expr-stage"));
        Ok(())
    }

    #[test]
    fn run_helper_executes_commands() -> crate::Result<()> {
        let temp = tempfile::tempdir()?;
        let file = temp.path().join("run-helper.txt");
        crate::qshr! {
            run pipeline!(sh(format!("echo via-run-helper > \"{}\"", file.display())) | "more");
        }?;
        let contents = std::fs::read_to_string(&file)?;
        assert!(contents.contains("via-run-helper"));
        Ok(())
    }
}
