use crate::scene::Shape;

/// Move an element with `name` to the provided `x`,`y` coordinates.
///
/// Returns `Ok(())` when the element was found and updated, otherwise an
/// `Err` with a message when the element does not exist.
pub fn move_element(shapes: &mut [Shape], name: &str, x: f32, y: f32) -> Result<(), String> {
    let mut found = false;
    for sh in shapes.iter_mut() {
        if sh.name() == name {
            match sh {
                Shape::Circle(c) => {
                    c.x = x;
                    c.y = y;
                    found = true;
                }
                Shape::Rect(r) => {
                    r.x = x;
                    r.y = y;
                    found = true;
                }
                Shape::Text(t) => {
                    t.x = x;
                    t.y = y;
                    found = true;
                }
                _ => {}
            }
        }
    }

    if found {
        Ok(())
    } else {
        Err(format!("Element '{}' not found", name))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::Shape;

    #[test]
    fn move_element_updates_position() {
        let mut shapes = vec![crate::shapes::circle::Circle::create_default("C".to_string())];
        assert!(move_element(&mut shapes, "C", 0.5, 0.25).is_ok());
        match &shapes[0] {
            Shape::Circle(c) => {
                assert!((c.x - 0.5).abs() < 1e-6);
                assert!((c.y - 0.25).abs() < 1e-6);
            }
            _ => panic!("expected circle"),
        }
    }

    #[test]
    fn move_element_not_found() {
        let mut shapes = vec![];
        assert!(move_element(&mut shapes, "Nope", 0.0, 0.0).is_err());
    }
}
