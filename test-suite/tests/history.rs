// Copyright (c) 2026 Faris Alfarhan
// SPDX-License-Identifier: GPL-3.0-only

use numcore_math::runtime_state::{CalcState, CalculatorMode};

fn submit(state: &mut CalcState, expression: &[u8]) {
    for &byte in expression {
        assert!(state.append_character_to_input(byte));
    }
    state.record_submission();
    state.clear_input();
}

#[test]
fn recalls_newest_first_and_clears_past_newest() {
    let mut state = CalcState::new();
    submit(&mut state, b"1+1");
    submit(&mut state, b"2+2");
    submit(&mut state, b"3+3");

    assert!(state.history_previous());
    assert_eq!(state.current_input(), b"3+3");
    assert!(state.history_previous());
    assert_eq!(state.current_input(), b"2+2");
    assert!(state.history_previous());
    assert_eq!(state.current_input(), b"1+1");
    assert!(state.history_previous());
    assert_eq!(state.current_input(), b"1+1");

    assert!(state.history_next());
    assert_eq!(state.current_input(), b"2+2");
    assert!(state.history_next());
    assert_eq!(state.current_input(), b"3+3");
    assert!(state.history_next());
    assert!(state.current_input().is_empty());
}

#[test]
fn ignores_duplicate_and_whitespace_submissions() {
    let mut state = CalcState::new();
    submit(&mut state, b"2+2");
    submit(&mut state, b"2+2");
    assert_eq!(state.history_len(), 1);

    submit(&mut state, b"   ");
    assert_eq!(state.history_len(), 1);
}

#[test]
fn keeps_eight_entries() {
    let mut state = CalcState::new();
    for value in 0u8..10 {
        let expression = [b'0' + value];
        submit(&mut state, &expression);
    }
    assert_eq!(state.history_len(), 8);
    for _ in 0..8 {
        assert!(state.history_previous());
    }
    assert_eq!(state.current_input(), b"2");
}

#[test]
fn clear_input_and_mode_change_leave_history() {
    let mut state = CalcState::new();
    submit(&mut state, b"7*7");
    assert!(state.history_previous());
    assert_eq!(state.current_input(), b"7*7");

    state.clear_input();
    assert!(!state.history_next());

    assert!(state.history_previous());
    state.switch_mode(CalculatorMode::Advanced);
    assert!(!state.history_next());
    assert!(state.current_input().is_empty());
}

#[test]
fn editing_a_recalled_entry_keeps_the_text() {
    let mut state = CalcState::new();
    submit(&mut state, b"12");
    assert!(state.history_previous());
    state.leave_history();
    assert!(state.append_character_to_input(b'3'));
    assert_eq!(state.current_input(), b"123");
}

#[test]
fn empty_history_does_nothing() {
    let mut state = CalcState::new();
    assert!(!state.history_previous());
    assert!(!state.history_next());
    assert!(state.current_input().is_empty());
}
