// examples1: how to use `public use fun`
module a::examples1 {
    use a::shapes::{Rectangle, Box, rectangle_base};

    // Example using a public use fun
    fun example(rectangle: &Rectangle, box: &Box): u64 {
        (rectangle.base() * rectangle.height()) +
        (box.base() * box.height() * box.depth())
    }

    // Same example but with the method calls expanded
    fun expanded_example(rectangle: &Rectangle, box: &Box): u64 {
        rectangle_base(rectangle)
        
    }
}
