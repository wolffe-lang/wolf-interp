//! is61 — wolffe-lang/wolf-interp#155: W1002 ("`mut` parameter the body never
//! writes") where the body moves the parameter out.
//!
//! The compiler raises E1001 at a move out of a `mut` parameter that reaches a
//! return (`[mem.tier0.mode.mut]`, s184, wolffe-lang/wolf-lang#464) and stands
//! W1002 down beside it: the move is the write its syntactic scan cannot see.
//! It keeps W1002 where the moved place is used again in the body (that move
//! drew its own E1001, which names no parameter), and it skips a parameter
//! whose name the body rebinds. This machine traps at run and has no static
//! E1001, so its lint reads the same evidence off the body (`src/lint.rs`,
//! `consumed_mut_params`).
//!
//! One directory per shape under `tests/lint_is61/`. Each row pins the
//! lupin verdict (trunk `8e2516d`'s — the lint moves no verdict) and the
//! warnings array wolf 0.2.19 answered on the same program
//! (`wolf conform-run main.lu --checked --json`, kasumi
//! `~/lanes/is61/evidence/probes-lupin-0.1.42.log`). lupin must answer that
//! array exactly: codes and spans, every code, not only W1002. The one shape
//! where the two machines part is its own test, below the table.

use std::path::{Path, PathBuf};

use wolf_interp::protocol::Warning;

fn program(name: &str) -> (PathBuf, Vec<u8>) {
    let file = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("lint_is61")
        .join(name)
        .join("main.lu");
    let source = std::fs::read(&file).expect("the shape carries its main.lu");
    (file, source)
}

fn observe(name: &str) -> (String, Vec<(String, [u64; 2])>) {
    let (file, source) = program(name);
    let (record, _) = wolf_interp::observe_record(&file, &source, None);
    let warnings: &[Warning] = record
        .warnings
        .as_deref()
        .unwrap_or_else(|| panic!("{name}: the analyses did not run"));
    let mut warnings: Vec<(String, [u64; 2])> =
        warnings.iter().map(|w| (w.code.clone(), w.span)).collect();
    warnings.sort();
    (record.verdict.to_string(), warnings)
}

fn check(name: &str, verdict: &str, compiler: &[(&str, [u64; 2])]) {
    let (observed_verdict, observed) = observe(name);
    let mut expected: Vec<(String, [u64; 2])> = compiler
        .iter()
        .map(|(code, span)| ((*code).to_owned(), *span))
        .collect();
    expected.sort();
    assert_eq!(
        observed, expected,
        "{name}: lupin warned {observed:?}, wolf 0.2.19 warns {expected:?}"
    );
    assert_eq!(
        observed_verdict, verdict,
        "{name}: the verdict moved — the lint must not change what runs"
    );
}

macro_rules! shapes {
    ($($name:ident: $verdict:literal, $compiler:expr;)*) => {
        $(
            #[test]
            fn $name() {
                check(stringify!($name), $verdict, $compiler);
            }
        )*
    };
}

shapes! {
    a01_row_whole: "trap(use-after-move)", &[];
    a02_row_elem: "trap(use-after-move)", &[];
    a03_row_field: "trap(use-after-move)", &[];
    a04_row_map: "trap(use-after-move)", &[];
    a05_row_one_path: "trap(use-after-move)", &[];
    m01_move_tail: "exit(0)", &[];
    m02_move_return: "exit(0)", &[];
    m03_move_into_arg: "exit(0)", &[("W0308", [141, 147])];
    m04_move_copy_param: "trap(use-after-move)", &[];
    m05_move_copy_field: "trap(use-after-move)", &[];
    m06_move_into_struct_lit: "trap(use-after-move)", &[];
    m07_move_into_list_lit: "trap(use-after-move)", &[];
    m08_move_store_back: "exit(0)", &[];
    m09_move_then_use_in_body: "trap(use-after-move)", &[("W1002", [5, 8])];
    m10_move_one_branch: "exit(0)", &[];
    m11_move_nested_elem: "exit(0)", &[];
    m12_move_self_field: "trap(use-after-move)", &[];
    m13_move_field_of_elem: "exit(0)", &[];
    m14_take_store_whole: "exit(0)", &[];
    m15_take_store_elem: "exit(0)", &[];
    m16_take_arg_onward: "exit(0)", &[("W0308", [146, 152])];
    m17_move_in_loop_store_back: "exit(0)", &[];
    m19_move_grouped: "trap(use-after-move)", &[];
    m20_move_str_param: "trap(use-after-move)", &[];
    p01_plain_let_whole: "trap(use-after-move)", &[];
    p02_plain_let_field: "trap(use-after-move)", &[];
    p03_plain_let_elem: "trap(use-after-move)", &[];
    p04_plain_tail: "exit(0)", &[];
    p05_plain_return: "exit(0)", &[];
    p06_plain_struct_lit: "trap(use-after-move)", &[];
    p07_plain_list_lit: "trap(use-after-move)", &[];
    p08_plain_read_arg: "exit(0)", &[("W1002", [44, 47]), ("W0308", [136, 142])];
    p09_plain_copy_field: "exit(0)", &[("W1002", [43, 46])];
    p10_plain_str: "exit(0)", &[("W1002", [5, 8])];
    p11_plain_scalar_struct: "trap(use-after-move)", &[];
    p12_plain_assign: "trap(use-after-move)", &[];
    p13_plain_elem_copy: "exit(0)", &[("W1002", [5, 8])];
    p14_plain_field_of_elem: "exit(0)", &[];
    p15_plain_generic: "exit(0)", &[];
    q02_map_try: "exit(0)", &[];
    q03_map_else_copy: "exit(0)", &[("W1002", [5, 8])];
    q04_map_get: "unsupported", &[("W1002", [5, 8])];
    q05_map_else_str: "exit(0)", &[("W1002", [5, 8])];
    q06_map_in_expr: "exit(0)", &[];
    q07_map_struct_value: "exit(0)", &[];
    q08_map_field: "exit(0)", &[];
    q09_map_member_len: "exit(0)", &[];
    // is67 (`[type.row.match]`, ruling #21): `match m[k] { ok(v) => …, none => 0 }` is
    // a match over a `List[int] ! {none}` whose value half no arm covers — `ok(v)` fits no
    // list and `none` is a row arm — so the value is refused by name where 0.1.43 bound the
    // list to `none` and printed `0`. The warnings are unchanged; the verdict moves with the
    // ruling, not with the lint (`docs/divergence-log.md`, is67 §3a).
    q10_map_match: "unsupported", &[("W1002", [5, 8])];
    r01_only_member_read: "exit(0)", &[("W1002", [5, 8]), ("W0308", [98, 104])];
    r02_copy_out: "exit(0)", &[("W1002", [5, 8])];
    r03_move_other_local: "exit(0)", &[("W1002", [5, 8]), ("W0308", [153, 159])];
    r04_shadow_then_move: "exit(0)", &[];
    r05_take_param_tail: "exit(0)", &[("W1003", [5, 9])];
    r06_take_param_move_tail: "exit(0)", &[];
    r07_copy_then_move_copy: "unsupported", &[("W1002", [5, 8])];
    r08_move_len_member: "unsupported", &[];
    s01_tuple_elem: "trap(use-after-move)", &[];
    s02_if_tail_value: "exit(0)", &[];
    s03_eq_operand: "exit(0)", &[("W1002", [5, 8]), ("W0308", [102, 108])];
    s04_interp_whole: "exit(0)", &[("W1002", [5, 8])];
    s05_for_over: "exit(0)", &[("W1002", [5, 8])];
    s06_match_scrutinee: "exit(0)", &[("W1002", [5, 8])];
    s07_plain_index_store_rhs: "exit(0)", &[("W1002", [29, 32])];
    s08_push_plain_arg: "exit(0)", &[("W1002", [29, 32])];
    s09_field_store_rhs: "trap(use-after-move)", &[];
    s10_slice_read: "exit(0)", &[("W1002", [5, 8])];
    s11_method_receiver: "exit(0)", &[("W1002", [5, 8])];
    s12_count_method: "exit(0)", &[("W1002", [5, 8])];
    t02_take_param_rebound: "exit(0)", &[];
    t04_for_pattern_rebind: "exit(0)", &[];
    t05_match_arm_rebind: "exit(0)", &[];
    t06_defer_read_after_move: "trap(use-after-move)", &[("W1002", [5, 8])];
    t07_match_arm_value: "exit(0)", &[];
    t10_map_readout_then_whole_read: "trap(use-after-move)", &[("W1002", [52, 55])];
    t11_generic_elem: "exit(0)", &[];
    t14_self_plain_field_tail: "exit(0)", &[];
    t15_move_in_while_break: "trap(use-after-move)", &[];
    // wolf-interp#159 (is62): the shorthand is its longhand and moves, as
    // wolf-lang trunk `57805e35` reads it (s190: `fail(E1001)`, no warning);
    // wolf 0.2.19 answered W1002 [33, 36] and ran.
    t16_struct_shorthand: "trap(use-after-move)", &[];
    t17_view_set_self: "exit(0)", &[];
    t18_nested_fn_param_name: "trap(use-after-move)", &[];
    t19_move_copy_elem_list: "exit(0)", &[];
    t20_str_elem: "fail(E0411)", &[("W1002", [5, 8])];
    u01_header_after_elem_move: "exit(0)", &[];
    u02_sibling_elem_after_move: "exit(0)", &[];
    u03_same_elem_after_move: "trap(use-after-move)", &[("W1002", [5, 8])];
    u04_use_in_other_branch: "exit(0)", &[];
    u05_move_in_loop: "exit(0)", &[("W1002", [5, 8])];
    u06_double_move: "trap(use-after-move)", &[];
    u07_use_before_move_in_loop: "exit(0)", &[("W1002", [5, 8])];
    u08_move_then_return_early: "exit(0)", &[];
    u09_field_move_sibling_read: "exit(0)", &[];
    u10_field_move_whole_read: "trap(use-after-move)", &[("W1002", [88, 91])];
    u11_move_two_params_one_used: "trap(use-after-move)", &[("W1002", [5, 8])];
    u12_move_after_loop_break: "trap(use-after-move)", &[];
    u13_move_dyn_index_then_other: "exit(0)", &[("W1002", [5, 8])];
    u14_try_exit_after_move: "trap(use-after-move)", &[("W1002", [35, 38])];
    u15_map_readout_then_len: "exit(0)", &[];
    u16_shadow_param_other_scope: "trap(use-after-move)", &[];
    u18_alias_int_param: "exit(0)", &[("W1002", [23, 26])];
    u19_enum_param: "exit(0)", &[];
    u20_tuple_param_elem: "exit(0)", &[];
    v01_nested_loop_break_out: "exit(0)", &[("W1002", [5, 8])];
    v02_continue_after_move: "exit(0)", &[("W1002", [5, 8])];
    v03_match_arm_move_other_arm_read: "exit(0)", &[];
    v04_defer_after_move: "trap(use-after-move)", &[("W1002", [5, 8])];
    v05_try_before_move: "exit(0)", &[];
    v06_self_whole_tail: "exit(0)", &[];
    v07_loop_cond_reads_moved: "exit(0)", &[("W1002", [5, 8])];
    v08_two_moves_one_whole_read: "trap(use-after-move)", &[];
    v09_move_after_infinite_loop: "exit(0)", &[("W1002", [5, 8])];
    v10_return_inside_loop: "exit(0)", &[];
    v11_else_handler_reads: "exit(0)", &[];
    v12_move_then_sibling_move: "exit(0)", &[];
    v13_for_body_move_break: "trap(use-after-move)", &[];
    v14_generic_param_whole: "exit(0)", &[];
    v15_str_elem_copy: "exit(0)", &[("W1002", [5, 8])];
    v16_move_into_map_store: "exit(0)", &[];
}

/// The one parting, healed at is68: a type error the compiler's typecheck
/// rung refuses (E0401) stops it before the mem rung, so the at-return E1001
/// that would stand W1002 down never runs and wolf 0.2.19 keeps
/// `W1002 [5, 8]`. Through 0.1.43 this machine had no refusal for the
/// annotation (it ran the body and trapped `use-after-move` at the caller's
/// read). Since is68 (wolf-interp#138) `let n: int = "a"` is the declared-
/// scalar pass's E0401 at the compiler's own span, `[83, 86]`, so both
/// machines refuse the program with one first error — `[proto.cmp.rung]`'s
/// agreement. This machine's lint says nothing, as before: the move reaches
/// the return, the at-return E1001's shape, which stands W1002 down.
#[test]
fn t01_type_error_beside_move() {
    let (verdict, warnings) = observe("t01_type_error_beside_move");
    assert_eq!(verdict, "fail(E0401)");
    assert!(
        warnings.is_empty(),
        "t01: lupin warned {warnings:?}; the move reaches the return unused"
    );
}
