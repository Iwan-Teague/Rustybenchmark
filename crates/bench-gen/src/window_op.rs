//! The `window-op` family (category `borrow-lifetimes`).
//!
//! Apply an in-place operation to selected consecutive, non-overlapping windows
//! of width `w` in a `&mut [i64]`, and return the number of windows the operation
//! was applied to. Solution-first: the seed selects the *operation* (Reverse /
//! RotateLeft(k) / RotateRight(k) / SwapEnds), a *stride* (every window / every
//! other window) and the function name, from which the reference, the example
//! outputs, the differential oracle and the skeleton are all derived — so seeds
//! vary the actual logic a model must produce, not just identifiers.

use crate::{mint_canary, GeneratedTask, Generator, Rng};
use std::collections::BTreeMap;
use std::path::PathBuf;

/// The per-seed operation.
#[derive(Clone, Debug, PartialEq)]
enum Op {
    Reverse,
    RotateLeft(usize),
    RotateRight(usize),
    SwapEnds,
}

struct Spec {
    op: Op,
    /// Apply to every `stride`-th window (1 = all, 2 = every other).
    stride: usize,
    fn_name: &'static str,
}

const NAMES: &[&str] = &[
    "apply_windows",
    "transform_chunks",
    "process_blocks",
    "rework_segments",
    "adjust_groups",
    "map_frames",
];

fn sample(seed: u64) -> Spec {
    let mut rng = Rng::new(seed);
    let op = match rng.below(4) {
        0 => Op::Reverse,
        1 => Op::RotateLeft(1 + rng.below(3) as usize), // k in 1..=3
        2 => Op::RotateRight(1 + rng.below(3) as usize), // k in 1..=3
        _ => Op::SwapEnds,
    };
    let stride = 1 + rng.below(2) as usize; // 1 or 2
    let fn_name = NAMES[rng.below(NAMES.len() as u64) as usize];
    Spec {
        op,
        stride,
        fn_name,
    }
}

/// Native reference, mirroring the emitted source exactly. Applies `op` to every
/// `stride`-th full window and returns how many were transformed.
fn apply(spec: &Spec, v: &mut [i64], w: usize) -> usize {
    if w == 0 {
        return 0;
    }
    let mut count = 0;
    let mut idx = 0;
    let mut i = 0;
    while i + w <= v.len() {
        if idx % spec.stride == 0 {
            let chunk = &mut v[i..i + w];
            match spec.op {
                Op::Reverse => chunk.reverse(),
                Op::RotateLeft(k) => chunk.rotate_left(k % w),
                Op::RotateRight(k) => chunk.rotate_right(k % w),
                Op::SwapEnds => {
                    let last = w - 1;
                    chunk.swap(0, last);
                }
            }
            count += 1;
        }
        idx += 1;
        i += w;
    }
    count
}

fn op_body(op: &Op) -> String {
    match op {
        Op::Reverse => "chunk.reverse();".to_string(),
        Op::RotateLeft(k) => format!("chunk.rotate_left({k} % chunk.len());"),
        Op::RotateRight(k) => format!("chunk.rotate_right({k} % chunk.len());"),
        Op::SwapEnds => {
            "let last = chunk.len() - 1;\n                chunk.swap(0, last);".to_string()
        }
    }
}

fn op_prose(op: &Op) -> String {
    match op {
        Op::Reverse => "reverse the elements of the window in place".to_string(),
        Op::RotateLeft(k) => {
            format!("rotate the elements of the window left by {k} position(s), wrapping around")
        }
        Op::RotateRight(k) => {
            format!("rotate the elements of the window right by {k} position(s), wrapping around")
        }
        Op::SwapEnds => "swap the first and last element of the window".to_string(),
    }
}

fn stride_prose(stride: usize) -> String {
    if stride == 1 {
        "For each consecutive, non-overlapping window of width `w`".to_string()
    } else {
        format!(
            "Number the consecutive, non-overlapping windows of width `w` from 0. \
             For every {stride}-th window (indices 0, {stride}, {}, …)",
            2 * stride
        )
    }
}

fn reference_src(spec: &Spec) -> String {
    format!(
        "pub fn {name}(v: &mut [i64], w: usize) -> usize {{\n\
         \x20   if w == 0 {{ return 0; }}\n\
         \x20   let mut count = 0;\n\
         \x20   for (idx, chunk) in v.chunks_exact_mut(w).enumerate() {{\n\
         \x20       if idx % {stride} == 0 {{\n\
         \x20           {body}\n\
         \x20           count += 1;\n\
         \x20       }}\n\
         \x20   }}\n\
         \x20   count\n\
         }}\n",
        name = spec.fn_name,
        stride = spec.stride,
        body = op_body(&spec.op),
    )
}

fn skeleton_src(spec: &Spec) -> String {
    let (examples, _) = worked_examples(spec);
    format!(
        "//! Implement `{name}` below.\n\
         //!\n\
         {doc}\n\
         pub fn {name}(v: &mut [i64], w: usize) -> usize {{\n\
         \x20   todo!()\n\
         }}\n",
        name = spec.fn_name,
        doc = examples
            .lines()
            .map(|l| format!("//! {l}"))
            .collect::<Vec<_>>()
            .join("\n"),
    )
}

/// One worked example: (input, width, expected output, expected count).
type ExampleCase = (Vec<i64>, usize, Vec<i64>, usize);

/// A few worked examples, computed natively so they are correct by construction.
fn worked_examples(spec: &Spec) -> (String, Vec<ExampleCase>) {
    // Enough windows that a stride of 2 is visible.
    let inputs: &[(&[i64], usize)] = &[
        (&[1, 2, 3, 4, 5, 6, 7, 8], 2),
        (&[1, 2, 3, 4, 5, 6], 2),
        (&[1, 2, 3, 4, 5, 6, 7, 8, 9], 3),
    ];
    let mut cases = Vec::new();
    let mut prose = String::new();
    for (input, w) in inputs {
        let mut out = input.to_vec();
        let count = apply(spec, &mut out, *w);
        prose.push_str(&format!(
            "  {input:?}, w={w}  ->  {out:?}, returns {count}\n"
        ));
        cases.push((input.to_vec(), *w, out, count));
    }
    (prose, cases)
}

fn prompt(spec: &Spec, canary: &str) -> String {
    let (examples, _) = worked_examples(spec);
    format!(
        "Implement the function `{name}` in `src/lib.rs`.\n\
         \n\
         {stride}, {op}. Any trailing elements that do not fill a full window are \
         left untouched, as are the windows you do not transform. Return the number \
         of windows the operation was applied to.\n\
         \n\
         Constraints:\n\
         - `w == 0` must return 0 and leave `v` unchanged (do not panic).\n\
         - Operate in place: do not allocate a second copy of the data.\n\
         - Do not use `unsafe`.\n\
         \n\
         Signature:\n\
         ```rust\n\
         pub fn {name}(v: &mut [i64], w: usize) -> usize\n\
         ```\n\
         \n\
         Examples:\n\
         {examples}\n\
         Return the complete contents of `src/lib.rs` as a single ```rust code block. \
         (ref: {canary})\n",
        name = spec.fn_name,
        stride = stride_prose(spec.stride),
        op = op_prose(&spec.op),
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

fn behavior_test_src(spec: &Spec) -> String {
    let (_, cases) = worked_examples(spec);
    let mut body = format!("use task::{};\n\n", spec.fn_name);
    for (i, (input, w, out, count)) in cases.iter().enumerate() {
        body.push_str(&format!(
            "#[test]\nfn ex{i}() {{\n\
             \x20   let mut v: Vec<i64> = vec!{input:?};\n\
             \x20   assert_eq!({name}(&mut v, {w}), {count});\n\
             \x20   assert_eq!(v, vec!{out:?});\n\
             }}\n\n",
            name = spec.fn_name,
        ));
    }
    body.push_str(&format!(
        "#[test]\nfn zero_width_noop() {{\n\
         \x20   let mut v = vec![1i64, 2, 3];\n\
         \x20   assert_eq!({name}(&mut v, 0), 0);\n\
         \x20   assert_eq!(v, vec![1, 2, 3]);\n\
         }}\n\n\
         #[test]\nfn empty_slice() {{\n\
         \x20   let mut v: Vec<i64> = Vec::new();\n\
         \x20   assert_eq!({name}(&mut v, 3), 0);\n\
         }}\n",
        name = spec.fn_name,
    ));
    body
}

fn differential_test_src(spec: &Spec) -> String {
    let reference =
        reference_src(spec).replacen(&format!("pub fn {}", spec.fn_name), "fn reference", 1);
    format!(
        "use task::{name};\n\
         \n\
         {reference}\n\
         #[test]\n\
         fn differential_vs_reference() {{\n\
         \x20   let mut state: u64 = 0x2545F4914F6CDD1D;\n\
         \x20   let mut next = || {{\n\
         \x20       state = state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);\n\
         \x20       (state >> 33) as u64\n\
         \x20   }};\n\
         \x20   for _ in 0..3000 {{\n\
         \x20       let len = (next() % 20) as usize;\n\
         \x20       let w = (next() % 6) as usize;\n\
         \x20       let mut a: Vec<i64> = (0..len).map(|_| (next() % 100) as i64 - 50).collect();\n\
         \x20       let mut b = a.clone();\n\
         \x20       let ra = {name}(&mut a, w);\n\
         \x20       let rb = reference(&mut b, w);\n\
         \x20       assert_eq!(rb, ra, \"count mismatch: len={{len}} w={{w}}\");\n\
         \x20       assert_eq!(b, a, \"array mismatch: len={{len}} w={{w}}\");\n\
         \x20   }}\n\
         }}\n",
        name = spec.fn_name,
        reference = reference,
    )
}

fn alloc_test_src(spec: &Spec) -> String {
    format!(
        "use task::{name};\n\
         use std::alloc::{{GlobalAlloc, Layout, System}};\n\
         use std::sync::atomic::{{AtomicUsize, Ordering}};\n\
         \n\
         static ALLOCS: AtomicUsize = AtomicUsize::new(0);\n\
         struct Counting;\n\
         unsafe impl GlobalAlloc for Counting {{\n\
         \x20   unsafe fn alloc(&self, l: Layout) -> *mut u8 {{ ALLOCS.fetch_add(1, Ordering::SeqCst); System.alloc(l) }}\n\
         \x20   unsafe fn dealloc(&self, p: *mut u8, l: Layout) {{ System.dealloc(p, l) }}\n\
         }}\n\
         #[global_allocator]\n\
         static GLOBAL: Counting = Counting;\n\
         \n\
         #[test]\n\
         fn hot_path_does_not_allocate() {{\n\
         \x20   let mut v: Vec<i64> = (0..256).collect();\n\
         \x20   let before = ALLOCS.load(Ordering::SeqCst);\n\
         \x20   let n = {name}(&mut v, 4);\n\
         \x20   let after = ALLOCS.load(Ordering::SeqCst);\n\
         \x20   assert!(n > 0);\n\
         \x20   assert_eq!(after - before, 0, \"allocated {{}} time(s)\", after - before);\n\
         }}\n",
        name = spec.fn_name,
    )
}

/// A degenerate answer for the trivial-baseline gate: correct signature, wrong
/// body. Each must fail grading.
fn const_zero(spec: &Spec) -> String {
    format!(
        "pub fn {name}(v: &mut [i64], w: usize) -> usize {{ let _ = (v, w); 0 }}\n",
        name = spec.fn_name,
    )
}
/// Counts windows but transforms nothing — must fail, since none of the ops is a
/// no-op.
fn identity(spec: &Spec) -> String {
    format!(
        "pub fn {name}(v: &mut [i64], w: usize) -> usize {{\n\
         \x20   if w == 0 {{ return 0; }}\n\
         \x20   let mut count = 0;\n\
         \x20   for (idx, _chunk) in v.chunks_exact_mut(w).enumerate() {{\n\
         \x20       if idx % {stride} == 0 {{ count += 1; }}\n\
         \x20   }}\n\
         \x20   count\n\
         }}\n",
        name = spec.fn_name,
        stride = spec.stride,
    )
}

pub struct WindowOpFamily;

impl Generator for WindowOpFamily {
    fn id(&self) -> &str {
        "window-op"
    }
    fn category(&self) -> &str {
        "borrow-lifetimes"
    }

    fn generate(&self, seed: u64) -> GeneratedTask {
        let spec = sample(seed);
        let canary = mint_canary("window-op", seed);

        let mut files = BTreeMap::new();
        files.insert(PathBuf::from("Cargo.toml"), cargo_toml());
        files.insert(PathBuf::from("src/lib.rs"), skeleton_src(&spec));

        let mut hidden = BTreeMap::new();
        hidden.insert(PathBuf::from("tests/behavior.rs"), behavior_test_src(&spec));
        hidden.insert(
            PathBuf::from("tests/differential.rs"),
            differential_test_src(&spec),
        );
        hidden.insert(PathBuf::from("tests/alloc.rs"), alloc_test_src(&spec));

        GeneratedTask {
            id: format!("window-op/{seed:016x}"),
            category: self.category().to_string(),
            prompt: prompt(&spec, &canary),
            canary,
            answer_path: "src/lib.rs".to_string(),
            files,
            hidden,
            behavior_test: "behavior".to_string(),
            differential_test: "differential".to_string(),
            alloc_test: "alloc".to_string(),
            max_unsafe: 0,
            forbidden_paths: Vec::new(),
            weights: (0.35, 0.55, 0.10),
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
            ("const-zero".to_string(), const_zero(&spec)),
            ("identity".to_string(), identity(&spec)),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generation_is_deterministic() {
        let g = WindowOpFamily;
        let a = g.generate(12345);
        let b = g.generate(12345);
        assert_eq!(a.prompt, b.prompt);
        assert_eq!(a.files, b.files);
        assert_eq!(a.hidden, b.hidden);
    }

    #[test]
    fn seeds_vary_op_and_stride() {
        let mut variants = std::collections::HashSet::new();
        for seed in 0..80u64 {
            let s = sample(seed);
            variants.insert(format!("{:?}/{}", s.op, s.stride));
        }
        assert!(
            variants.len() >= 10,
            "expected structural variety, got {}",
            variants.len()
        );
    }

    #[test]
    fn reference_agrees_with_native_apply() {
        for seed in [1u64, 2, 3, 7, 42, 99] {
            let spec = sample(seed);
            let (_, cases) = worked_examples(&spec);
            for (input, w, expected, count) in cases {
                let mut v = input.clone();
                let c = apply(&spec, &mut v, w);
                assert_eq!((c, v), (count, expected), "seed {seed}");
            }
        }
    }

    #[test]
    fn canary_is_in_the_prompt() {
        let g = WindowOpFamily;
        let t = g.generate(42);
        assert!(t.prompt.contains(&t.canary));
    }
}
