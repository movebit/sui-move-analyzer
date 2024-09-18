// This module is a demo on "method syntax"
// [move 2024][alpha] Method syntax #13933
// you can view this PR for more details:
// https://github.com/MystenLabs/sui/pull/13933
module Move2024Demo::shapes {

    public struct Rectangle { base: u64, height: u64 }
    public struct Box { base: u64, height: u64, depth: u64 }
    public fun rectangle(base: u64, height: u64): Rectangle {
        Rectangle { base, height }
    }

    // Rectangle and Box can have methods with the same name

    public use fun rectangle_base as Rectangle.base;
    public fun rectangle_base(rectangle: &Rectangle): u64 {
        rectangle.base
    }

    public use fun rectangle_height as Rectangle.height;
    public fun rectangle_height(rectangle: &Rectangle): u64 {
        rectangle.height
    }

    public fun box(base: u64, height: u64, depth: u64): Box {
        Box { base, height, depth }
    }

    public use fun box_base as Box.base;
    public fun box_base(box: &Box): u64 {
        box.base
    }

    public use fun box_height as Box.height;
    public fun box_height(box: &Box): u64 {
        box.height
    }

    public use fun box_depth as Box.depth;
    public fun box_depth(box: &Box): u64 {
        box.depth
    }
}

// examples1: how to use `public use fun`
module Move2024Demo::examples1 {
    use Move2024Demo::shapes::{Rectangle, Box};

    // Example using a public use fun
    fun example(rectangle: &Rectangle, box: &Box): u64 {
        (rectangle.base() * rectangle.height()) +
        (box.base() * box.height() * box.depth())
    }

    // Same example but with the method calls expanded
    fun expanded_example(rectangle: &Rectangle, box: &Box): u64 {
        (Move2024Demo::shapes::rectangle_base(rectangle) *
        Move2024Demo::shapes::rectangle_height(rectangle)) +
        (Move2024Demo::shapes::box_base(box) *
        Move2024Demo::shapes::box_height(box) *
        Move2024Demo::shapes::box_depth(box))
    }
}

// examples2: how to use `use fun`
module Move2024Demo::examples2 {
    use Move2024Demo::shapes::{Rectangle, Box};

    use fun into_box as Rectangle.into_box;
    fun into_box(rectangle: &Rectangle, depth: u64): Box {
        Move2024Demo::shapes::box(rectangle.base(), rectangle.height(), depth)
    }

    // Example using a local use fun
    fun example(rectangle: &Rectangle): Box {
        rectangle.into_box(1)
    }

    // Same example but with the method calls expanded
    fun expanded_example(rectangle: &Rectangle): Box {
        into_box(rectangle, 1)
    }
}

// examples3: Uses Create Implicit Use Funs
module Move2024Demo::examples3 {
    use Move2024Demo::shapes::{Rectangle, Box};

    // Example using a local use fun
    fun example(rectangle: &Rectangle): u64 {
        use Move2024Demo::shapes::{rectangle_base as b, rectangle_height as h};
        // implicit 'use fun Move2024Demo::shapes::rectangle_base as Rectangle.b'
        // implicit 'use fun Move2024Demo::shapes::rectangle_height as Rectangle.h'
        rectangle.b() * rectangle.h()
    }

    // Same example but with the method calls expanded
    fun expanded_example(rectangle: &Rectangle): u64 {
        Move2024Demo::shapes::rectangle_base(rectangle) * 
        Move2024Demo::shapes::rectangle_height(rectangle)
    }
}


// auto expanded borrow; auto type inference
// [move-compiler] Add public struct type support to the parser #13917
// https://github.com/MystenLabs/sui/pull/13917
module Move2024Demo::cup {
    public struct Cup<T> { value: T }

    public fun borrow<T>(cup: &Cup<T>): &T { &cup.value }
    public fun borrow_mut<T>(cup: &mut Cup<T>): &mut T { &mut cup.value }
    public fun value<T>(cup: Cup<T>): T { let Cup { value } = cup; value }
}

module Move2024Demo::examples {
    use Move2024Demo::cup::Cup;

    // Examples showing the three cases for how a value is used
    fun examples<T>(mut cup: Cup<T>): T {
        // The type annotations are not necessary, but here for clarity.
        // automatic immutable borrow
        let _: &T = cup.borrow();
        // automatic mutable borrow
        let _: &mut T = cup.borrow_mut();
        // no borrow needed
        cup.value()
    }

    fun expanded_examples<T>(mut cup: Cup<T>): T {
        let _: &T = Move2024Demo::cup::borrow(&cup);
        let _: &mut T =  Move2024Demo::cup::borrow_mut(&mut cup);
        Move2024Demo::cup::value(cup)
    }
}
