//! The `error-handling` family (category `error-handling`).
//!
//! A deliberately different task shape from `window-op`, to test whether
//! solution-first seeding generalises beyond in-place mutation. The task: parse
//! a slice of `&str` items into `i64`, validate each against a seed-selected
//! rule, and combine them with a seed-selected operation, returning
//! `Result<i64, ParseError>` — a parse failure or a rule failure short-circuits.
//! The interesting content is the *error paths* and `?` propagation.
//!
//! The constraint is error-handling-specific and reuses the AST forbidden-path
//! check (now matching method calls): no `.unwrap()` / `.expect()` — you must
//! propagate errors, not panic.

use crate::{mint_canary, GeneratedTask, Generator, Rng};
use std::collections::BTreeMap;
use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq)]
enum Combine {
    Sum,
    Product,
    SumAbs,
}

#[derive(Clone, Debug, PartialEq)]
enum Rule {
    NonNegative,
    AtMost(i64),
    NonZero,
}

struct Spec {
    combine: Combine,
    rule: Rule,
    fn_name: &'static str,
}

const NAMES: &[&str] = &[
    "reduce_items",
    "fold_values",
    "accumulate",
    "combine_entries",
    "aggregate",
    "tally",
];

fn sample(seed: u64) -> Spec {
    let mut rng = Rng::new(seed);
    let combine = match rng.below(3) {
        0 => Combine::Sum,
        1 => Combine::Product,
        _ => Combine::SumAbs,
    };
    let rule = match rng.below(3) {
        0 => Rule::NonNegative,
        1 => Rule::AtMost([10, 50, 100][rng.below(3) as usize]),
        _ => Rule::NonZero,
    };
    let fn_name = NAMES[rng.below(NAMES.len() as u64) as usize];
    Spec {
        combine,
        rule,
        fn_name,
    }
}

fn init(c: &Combine) -> i64 {
    match c {
        Combine::Sum | Combine::SumAbs => 0,
        Combine::Product => 1,
    }
}

/// Native combine, mirroring the emitted `acc = …`.
fn step(c: &Combine, acc: i64, n: i64) -> i64 {
    match c {
        Combine::Sum => acc + n,
        Combine::Product => acc * n,
        Combine::SumAbs => acc + n.abs(),
    }
}

fn passes(rule: &Rule, n: i64) -> bool {
    match rule {
        Rule::NonNegative => n >= 0,
        Rule::AtMost(b) => n <= *b,
        Rule::NonZero => n != 0,
    }
}

/// The native reference outcome: `Ok(acc)`, or the first error encountered.
/// `Ok` values are compared as `i64`; errors as a tag string, matching how the
/// emitted `ParseError` derives `PartialEq`.
fn eval(spec: &Spec, items: &[&str]) -> Result<i64, String> {
    let mut acc = init(&spec.combine);
    for it in items {
        let n: i64 = it.parse().map_err(|_| format!("NotANumber({it})"))?;
        if !passes(&spec.rule, n) {
            return Err(format!("FailedRule({n})"));
        }
        acc = step(&spec.combine, acc, n);
    }
    Ok(acc)
}

fn acc_expr(c: &Combine) -> &'static str {
    match c {
        Combine::Sum => "acc + n",
        Combine::Product => "acc * n",
        Combine::SumAbs => "acc + n.abs()",
    }
}

fn rule_cond(rule: &Rule) -> String {
    match rule {
        Rule::NonNegative => "n >= 0".to_string(),
        Rule::AtMost(b) => format!("n <= {b}"),
        Rule::NonZero => "n != 0".to_string(),
    }
}

fn combine_prose(c: &Combine) -> &'static str {
    match c {
        Combine::Sum => "their sum",
        Combine::Product => "their product",
        Combine::SumAbs => "the sum of their absolute values",
    }
}

fn rule_prose(rule: &Rule) -> String {
    match rule {
        Rule::NonNegative => "be non-negative (>= 0)".to_string(),
        Rule::AtMost(b) => format!("be at most {b}"),
        Rule::NonZero => "be non-zero".to_string(),
    }
}

const ENUM_SRC: &str = "#[derive(Debug, PartialEq)]\n\
     pub enum ParseError {\n\
     \x20   NotANumber(String),\n\
     \x20   FailedRule(i64),\n\
     }\n";

fn reference_src(spec: &Spec) -> String {
    format!(
        "{enum_src}\n\
         pub fn {name}(items: &[&str]) -> Result<i64, ParseError> {{\n\
         \x20   let mut acc: i64 = {init};\n\
         \x20   for item in items {{\n\
         \x20       let n: i64 = item.parse().map_err(|_| ParseError::NotANumber((*item).to_string()))?;\n\
         \x20       if !({cond}) {{\n\
         \x20           return Err(ParseError::FailedRule(n));\n\
         \x20       }}\n\
         \x20       acc = {acc};\n\
         \x20   }}\n\
         \x20   Ok(acc)\n\
         }}\n",
        enum_src = ENUM_SRC,
        name = spec.fn_name,
        init = init(&spec.combine),
        cond = rule_cond(&spec.rule),
        acc = acc_expr(&spec.combine),
    )
}

fn skeleton_src(spec: &Spec) -> String {
    let examples = worked_examples_prose(spec);
    format!(
        "//! Implement `{name}` below.\n\
         //!\n\
         {doc}\n\
         {enum_src}\n\
         pub fn {name}(items: &[&str]) -> Result<i64, ParseError> {{\n\
         \x20   todo!()\n\
         }}\n",
        name = spec.fn_name,
        enum_src = ENUM_SRC,
        doc = examples
            .lines()
            .map(|l| format!("//! {l}"))
            .collect::<Vec<_>>()
            .join("\n"),
    )
}

/// (label, items, expected) worked examples, computed natively.
fn worked_examples(spec: &Spec) -> Vec<(Vec<&'static str>, Result<i64, String>)> {
    let inputs: Vec<Vec<&'static str>> = vec![
        vec!["1", "2", "3"],
        vec!["4", "5"],
        vec!["7", "oops", "9"], // parse failure
        vec![],
    ];
    inputs
        .into_iter()
        .map(|items| (items.clone(), eval(spec, &items)))
        .collect()
}

fn worked_examples_prose(spec: &Spec) -> String {
    let mut s = String::new();
    for (items, res) in worked_examples(spec) {
        let r = match res {
            Ok(v) => format!("Ok({v})"),
            Err(e) => format!("Err(ParseError::{e})"),
        };
        s.push_str(&format!("  {items:?}  ->  {r}\n"));
    }
    s
}

fn prompt(spec: &Spec, canary: &str) -> String {
    let examples = worked_examples_prose(spec);
    format!(
        "Implement the function `{name}` in `src/lib.rs`. The `ParseError` enum is \
         already provided; keep it.\n\
         \n\
         Parse each item of `items` as an `i64`. If an item does not parse, return \
         `Err(ParseError::NotANumber(item))`. Each parsed value must {rule}; if one \
         does not, return `Err(ParseError::FailedRule(value))`. Otherwise return \
         `Ok` of {combine}. An empty input returns `Ok({init})`.\n\
         \n\
         Constraints:\n\
         - Propagate errors; do not `.unwrap()`, `.expect()` or panic.\n\
         - Do not use `unsafe`.\n\
         \n\
         Signature:\n\
         ```rust\n\
         pub fn {name}(items: &[&str]) -> Result<i64, ParseError>\n\
         ```\n\
         \n\
         Examples:\n\
         {examples}\n\
         Return the complete contents of `src/lib.rs` as a single ```rust code block. \
         (ref: {canary})\n",
        name = spec.fn_name,
        rule = rule_prose(&spec.rule),
        combine = combine_prose(&spec.combine),
        init = init(&spec.combine),
        examples = examples,
    )
}

fn cargo_toml() -> String {
    "[package]\n\
     name = \"task\"\n\
     version = \"0.0.0\"\n\
     edition = \"2021\"\n\
     \n\
     [lib]\n\
     path = \"src/lib.rs\"\n\
     \n\
     [workspace]\n"
        .to_string()
}

/// Render an expected outcome as a Rust `Result` literal for the tests.
fn render_expected(res: &Result<i64, String>) -> String {
    match res {
        Ok(v) => format!("Ok({v})"),
        Err(e) => {
            if let Some(inner) = e
                .strip_prefix("NotANumber(")
                .and_then(|r| r.strip_suffix(')'))
            {
                format!("Err(ParseError::NotANumber(String::from({inner:?})))")
            } else if let Some(inner) = e
                .strip_prefix("FailedRule(")
                .and_then(|r| r.strip_suffix(')'))
            {
                format!("Err(ParseError::FailedRule({inner}))")
            } else {
                unreachable!("unexpected error tag: {e}")
            }
        }
    }
}

fn behavior_test_src(spec: &Spec) -> String {
    let mut body = format!("use task::{{ParseError, {}}};\n\n", spec.fn_name);
    for (i, (items, res)) in worked_examples(spec).iter().enumerate() {
        let items_lit = format!("{items:?}");
        body.push_str(&format!(
            "#[test]\nfn ex{i}() {{\n\
             \x20   let items: Vec<&str> = vec!{items_lit};\n\
             \x20   assert_eq!({name}(&items), {expect});\n\
             }}\n\n",
            name = spec.fn_name,
            expect = render_expected(res),
        ));
    }
    body
}

fn differential_test_src(spec: &Spec) -> String {
    let reference = reference_src(spec).replacen(ENUM_SRC, "", 1).replacen(
        &format!("pub fn {}", spec.fn_name),
        "fn reference",
        1,
    );
    format!(
        "use task::{{ParseError, {name}}};\n\
         \n\
         {reference}\n\
         #[test]\n\
         fn differential_vs_reference() {{\n\
         \x20   let mut state: u64 = 0x9E3779B97F4A7C15;\n\
         \x20   let mut next = || {{\n\
         \x20       state = state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);\n\
         \x20       (state >> 33) as u64\n\
         \x20   }};\n\
         \x20   for _ in 0..3000 {{\n\
         \x20       let count = (next() % 5) as usize;\n\
         \x20       let mut owned: Vec<String> = Vec::new();\n\
         \x20       for _ in 0..count {{\n\
         \x20           if next() % 5 == 0 {{\n\
         \x20               owned.push(\"nope\".to_string());\n\
         \x20           }} else {{\n\
         \x20               let v = (next() % 220) as i64 - 110;\n\
         \x20               owned.push(v.to_string());\n\
         \x20           }}\n\
         \x20       }}\n\
         \x20       let items: Vec<&str> = owned.iter().map(|s| s.as_str()).collect();\n\
         \x20       assert_eq!({name}(&items), reference(&items), \"mismatch on {{items:?}}\");\n\
         \x20   }}\n\
         }}\n",
        name = spec.fn_name,
        reference = reference,
    )
}

/// A degenerate answer for the trivial-baseline gate.
fn const_ok(spec: &Spec) -> String {
    format!(
        "{enum_src}\n\
         pub fn {name}(items: &[&str]) -> Result<i64, ParseError> {{ let _ = items; Ok(0) }}\n",
        enum_src = ENUM_SRC,
        name = spec.fn_name,
    )
}
/// Uses `.unwrap()` instead of propagating — must fail (panics on bad input, and
/// violates the forbidden-path constraint).
fn unwrap_version(spec: &Spec) -> String {
    format!(
        "{enum_src}\n\
         pub fn {name}(items: &[&str]) -> Result<i64, ParseError> {{\n\
         \x20   let mut acc: i64 = {init};\n\
         \x20   for item in items {{\n\
         \x20       let n: i64 = item.parse().unwrap();\n\
         \x20       acc = {acc};\n\
         \x20   }}\n\
         \x20   Ok(acc)\n\
         }}\n",
        enum_src = ENUM_SRC,
        name = spec.fn_name,
        init = init(&spec.combine),
        acc = acc_expr(&spec.combine),
    )
}

pub struct ErrorHandlingFamily;

impl Generator for ErrorHandlingFamily {
    fn id(&self) -> &str {
        "error-handling"
    }
    fn category(&self) -> &str {
        "error-handling"
    }

    fn generate(&self, seed: u64) -> GeneratedTask {
        let spec = sample(seed);
        let canary = mint_canary("error-handling", seed);

        let mut files = BTreeMap::new();
        files.insert(PathBuf::from("Cargo.toml"), cargo_toml());
        files.insert(PathBuf::from("src/lib.rs"), skeleton_src(&spec));

        let mut hidden = BTreeMap::new();
        hidden.insert(PathBuf::from("tests/behavior.rs"), behavior_test_src(&spec));
        hidden.insert(
            PathBuf::from("tests/differential.rs"),
            differential_test_src(&spec),
        );

        GeneratedTask {
            id: format!("error-handling/{seed:016x}"),
            category: self.category().to_string(),
            prompt: prompt(&spec, &canary),
            canary,
            answer_path: "src/lib.rs".to_string(),
            files,
            hidden,
            behavior_test: "behavior".to_string(),
            differential_test: "differential".to_string(),
            // No allocation constraint: parsing legitimately allocates.
            alloc_test: String::new(),
            max_unsafe: 0,
            forbidden_paths: vec!["unwrap".to_string(), "expect".to_string()],
            // error-handling is behaviour-emphasis (docs/04 default weights).
            weights: (0.70, 0.20, 0.10),
        }
    }

    fn reference_code(&self, seed: u64) -> String {
        reference_src(&sample(seed))
    }
    fn skeleton_code(&self, seed: u64) -> String {
        skeleton_src(&sample(seed))
    }
    fn trivial_baselines(&self, seed: u64) -> Vec<(String, String)> {
        let spec = sample(seed);
        vec![
            ("const-ok".to_string(), const_ok(&spec)),
            ("unwrap".to_string(), unwrap_version(&spec)),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generation_is_deterministic() {
        let g = ErrorHandlingFamily;
        assert_eq!(g.generate(7).prompt, g.generate(7).prompt);
        assert_eq!(g.generate(7).hidden, g.generate(7).hidden);
    }

    #[test]
    fn seeds_vary_combine_and_rule() {
        let mut variants = std::collections::HashSet::new();
        for seed in 0..80u64 {
            let s = sample(seed);
            variants.insert(format!("{:?}/{:?}", s.combine, s.rule));
        }
        assert!(
            variants.len() >= 8,
            "expected variety, got {}",
            variants.len()
        );
    }

    #[test]
    fn eval_matches_intent() {
        let spec = Spec {
            combine: Combine::Sum,
            rule: Rule::NonNegative,
            fn_name: "f",
        };
        assert_eq!(eval(&spec, &["1", "2", "3"]), Ok(6));
        assert_eq!(eval(&spec, &["1", "-2"]), Err("FailedRule(-2)".to_string()));
        assert_eq!(eval(&spec, &["x"]), Err("NotANumber(x)".to_string()));
        assert_eq!(eval(&spec, &[]), Ok(0));
    }

    #[test]
    fn canary_is_in_the_prompt() {
        let g = ErrorHandlingFamily;
        let t = g.generate(3);
        assert!(t.prompt.contains(&t.canary));
    }
}
