module a::shapes {

    public struct Rectangle { base: u64, height: u64 }
    public struct Box { base: u64, height: u64, depth: u64 }
    public fun rectangle(base: u64, height: u64): Rectangle {
        Rectangle { base, height }
    }

    // Rectangle and Box can have methods with the same name

    // public use fun rectangle_base as Rectangle.base;
    public fun rectangle_base(rectangle: &Rectangle): u64 {
        rectangle.base
    }

    // public use fun rectangle_height as Rectangle.height;
    public fun rectangle_height(rectangle: &Rectangle): u64 {
        rectangle.height
    }

    public fun box(base: u64, height: u64, depth: u64): Box {
        Box { base, height, depth }
    }

    // public use fun box_base as Box.base;
    public fun box_base(box: &Box): u64 {
        1;
        box.base
    }

    // public use fun box_height as Box.height;
    public fun box_height(box: &Box): u64 {
        box.height
    }

    // public use fun box_depth as Box.depth;
    public fun box_depth(box: &Box): u64 {
        box.depth
    }
}
