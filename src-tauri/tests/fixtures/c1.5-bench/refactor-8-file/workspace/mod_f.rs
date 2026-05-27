// mod_f.rs — cross-file caller of old_name.
pub fn caller_f(x: i32) -> i32 {
    crate::mod_a::old_name(x) + 6
}

pub fn helper_f(x: i32) -> i32 {
    caller_f(x) * 6
}
