// This module is a demo on "positional struct fields syntax" and "postfix ability"
// [move-compiler] Add positional struct fields #14073
// [move-compiler] Add support for postfix ability declarations #13782

// you can view this PR for more details:
// https://github.com/MystenLabs/sui/pull/14073
// https://github.com/MystenLabs/sui/pull/13782
module Move2024Demo::M22 {
    public struct Foo(u64) has copy, drop;  // Postfix Ability Declaration

    fun x() {
        let _x = Foo(0);
        abort 0
    }
}
