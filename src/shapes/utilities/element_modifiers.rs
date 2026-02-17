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
                Shape::Circle { x: sx, y: sy, .. } | Shape::Rect { x: sx, y: sy, .. } => {
                    *sx = x;
                    *sy = y;
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
        let mut shapes = vec![Shape::Circle {
            name: "C".to_string(),
            x: 0.0,
            y: 0.0,
            radius: 1.0,
            color: crate::shapes::circle::default_color(),
            spawn_time: 0.0,
            animations: Vec::new(),
            visible: true,
        }];
        assert!(move_element(&mut shapes, "C", 0.5, 0.25).is_ok());
        match &shapes[0] {
            Shape::Circle { x, y, .. } => {
                assert!((*x - 0.5).abs() < 1e-6);
                assert!((*y - 0.25).abs() < 1e-6);
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
