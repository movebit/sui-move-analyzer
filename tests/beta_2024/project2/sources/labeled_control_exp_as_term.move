// This module is a demo on "named blocks"
// [move compiler] add named blocks #14577
// you can view this PR for more details:
// https://github.com/MystenLabs/sui/pull/14577
module Move2024Demo::M222 {

    fun foo(_: &u64) {}

    #[allow(dead_code)]
    fun t(cond: bool) { 'a: {
        1 + if (cond) 0 else 'a: { 1 } + 2;
        1 + 'a: loop {} + 2;
        1 + return 'a + 0;

        foo(&if (cond) 0 else 1);
        foo(&'a: loop {});
        foo(&return 'a);
        foo(&abort 0);
    } }
}
