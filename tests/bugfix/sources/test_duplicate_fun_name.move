module a::test_duplicate_fun_name1 {
    public fun foo() {}
    public fun bar() {}
}

module a::test_duplicate_fun_name2 {
    fun foo() {}
    public fun bar2() {}
}

module b::test_duplicate_fun_name {
    use a::test_duplicate_fun_name1::foo;
    use a::test_duplicate_fun_name1::bar;
    public fun test_goto_foo() {
        foo();
        // bug: can't find definition like a::b::fun_name()
        a::test_duplicate_fun_name1::bar();
        a::test_duplicate_fun_name2::bar2();

        // test duplicate function name in different modules
        a::test_duplicate_fun_name2::foo();
        a::test_duplicate_fun_name1::foo();
    }
}
