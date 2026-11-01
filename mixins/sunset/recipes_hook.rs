// Unlock All Recipes: a click on a recipe the server has not given you lays it in the grid with slot clicks instead.
fn sunset_place_recipe(game: &mut crate::game::Game<'_>, entry_index: usize) -> std::ops::ControlFlow<()> {
    if crate::game::recipe_book::sunset_recipes::place(game, entry_index) {
        std::ops::ControlFlow::Break(())
    } else {
        std::ops::ControlFlow::Continue(())
    }
}
