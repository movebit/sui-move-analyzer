module a::shapes {

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

// Public Use Fun
//这部分代码展示了如何使用 public use fun 来创建公共的方法别名。这些别名允许在不同的模块中以不同的名称调用相同的函数。在这个例子中，
//Rectangle 和 Box 结构体有相同名称的方法（base 和 height），但是通过 public use fun，我们可以为它们创建不同的别名，然后在
 //example 函数中使用这些别名来访问结构体的字段。
module b::examples1 {
    use a::shapes::{Rectangle, Box};

    // Example using a public use fun
    fun example(rectangle: &Rectangle, box: &Box): u64 {
        (rectangle.base() * rectangle.height()) +
        (box.base() * box.height() * box.depth())
    }

    // Same example but with the method calls expanded
    fun expanded_example(rectangle: &Rectangle, box: &Box): u64 {
        (a::shapes::rectangle_base(rectangle) *
        a::shapes::rectangle_height(rectangle)) +
        (a::shapes::box_base(box) *
        a::shapes::box_height(box) *
        a::shapes::box_depth(box))
    }
}

// Use Fun
// 如何使用 use fun 来创建局部的方法别名。这些别名只在定义它们的模块中有效。在这个例子中，
// into_box 被定义为一个别名，它实际上是一个将 Rectangle 转换为 Box 的函数。
// 在 example 函数中，我们可以直接在 Rectangle 上调用 into_box 方法，而不是显式地调用 a::shapes::box 函数
module b::examples2 {
    use a::shapes::{Rectangle, Box};

    use fun into_box as Rectangle.into_box;
    fun into_box(rectangle: &Rectangle, depth: u64): Box {
        a::shapes::box(rectangle.base(), rectangle.height(), depth)
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

//  Uses Create Implicit Use Funs
// 在使用 use 导入函数时隐式创建方法别名。当函数的第一个参数是当前模块中定义的类型的引用时，Move 会自动创建一个方法别名。
// 在这个例子中，我们使用 use 导入了 rectangle_base 和 rectangle_height 函数，
// 并为它们创建了别名 b 和 h。然后我们可以在 example 函数中像调用方法一样使用这些别名
module b::examples3 {
    use a::shapes::{Rectangle, Box};

    // Example using a local use fun
    fun example(rectangle: &Rectangle): u64 {
        use a::shapes::{rectangle_base as b, rectangle_height as h};
        // implicit 'use fun a::shapes::rectangle_base as Rectangle.b'
        // implicit 'use fun a::shapes::rectangle_height as Rectangle.h'
        rectangle.b() * rectangle.h()
    }

    // Same example but with the method calls expanded
    fun expanded_example(rectangle: &Rectangle): u64 {
        a::shapes::rectangle_base(rectangle) * 
        a::shapes::rectangle_height(rectangle)
    }
}


// 扩展的自动借用和签名
module a::cup {
    public struct Cup<T> { value: T }

    public fun borrow<T>(cup: &Cup<T>): &T { &cup.value }
    public fun borrow_mut<T>(cup: &mut Cup<T>): &mut T { &mut cup.value }
    public fun value<T>(cup: Cup<T>): T { let Cup { value } = cup; value }
}

module b::examples {
    use a::cup::Cup;

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

    // 没有使用自动借用语法
    fun expanded_examples<T>(mut cup: Cup<T>): T {
        let _: &T = a::cup::borrow(&cup);
        let _: &mut T =  a::cup::borrow_mut(&mut cup);
        a::cup::value(cup)
    }
}
