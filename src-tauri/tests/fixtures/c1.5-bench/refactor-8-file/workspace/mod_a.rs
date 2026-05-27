// mod_a.rs — defines the symbol being renamed.
pub fn old_name(x: i32) -> i32 {
    x + 1
}

pub fn caller_a(x: i32) -> i32 {
    old_name(x) + 1
}
