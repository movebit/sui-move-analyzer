// This module is a demo on "named blocks" and "break with value"
// [move compiler] add named blocks #14577
// Cgswords/loop value break #14030

// you can view this PR for more details:
// https://github.com/MystenLabs/sui/pull/14577
// https://github.com/MystenLabs/sui/pull/14030
module Move2024Demo::m111 {
    // cases that need parens
    fun t(cond: bool): u64 {
        loop { break ('a: { 1 }) };
        loop { break ('a: loop { break 0 }) };
        if (cond) return ('a: { 1 });
        0
    }
    fun t2(cond: bool) {
        if (cond) return ('a: while (cond) {});
    }

    // misleading cases but valid
    fun t3(cond: bool) {
        'a: loop { break 'a { 1 } };
        'a: loop { break 'a loop { break 0 } };
        'a: { return 'a { 1 } };
        'a: { return 'a while (cond) {} };
    }
}
