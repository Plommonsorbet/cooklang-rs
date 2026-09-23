//! Recipe representation

use relative_path::RelativePathBuf;
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ReferenceError {
    #[error("reference is missing name {0}")]
    MissingName(String),
}

#[cfg(feature = "ts")]
use tsify::Tsify;

use crate::{
    convert::Converter, metadata::Metadata, parser::Modifiers, quantity::Quantity,
    semantic_eq::SemanticEq, GroupedQuantity,
};

/// A complete recipe
///
/// The recipes do not have a name. You give it externally or maybe use
/// some metadata key.
///
/// The recipe returned from parsing is a [`ScalableRecipe`].
///
/// The difference between [`ScalableRecipe`] and [`ScaledRecipe`] is in the
/// values of the quantities of ingredients, cookware and timers. The parser
/// returns [`ScalableValue`]s and after scaling, these are converted to regular
/// [`Value`]s.
#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
#[cfg_attr(feature = "ts", derive(Tsify))]
pub struct Recipe {
    /// Metadata as read from preamble
    #[cfg_attr(feature = "ts", serde(rename = "raw_metadata"))]
    pub metadata: Metadata,
    /// Each of the sections
    ///
    /// If no sections declared, a section without name
    /// is the default.
    pub sections: Vec<Section>,
    /// All the ingredients
    pub ingredients: Vec<Ingredient>,
    /// All the cookware
    pub cookware: Vec<Cookware>,
    /// All the timers
    pub timers: Vec<Timer>,
    /// All the inline quantities
    pub inline_quantities: Vec<Quantity>,
    /// The source for the recipe
    pub source: Option<RecipeReference>,
}

impl Recipe {
    /// Compares two recipes using unit-aware quantity comparison.
    ///
    /// Unlike [`PartialEq`], quantities are compared with [`Quantity::equals`],
    /// so `5 dl` equals `0.5 l` and `5 min` equals `300 s`.
    pub fn equals(&self, other: &Self, converter: &Converter) -> bool {
        SemanticEq::equals(self, other, converter)
    }
}

impl SemanticEq for Recipe {
    fn equals(&self, other: &Self, converter: &Converter) -> bool {
        // Specifically does not compare recipe.source as it is
        // runtime dependent and does not have an "equivalent".
        self.metadata.equals(&other.metadata, converter)
            && self.sections == other.sections
            && self.ingredients.equals(&other.ingredients, converter)
            && self.cookware.equals(&other.cookware, converter)
            && self.timers.equals(&other.timers, converter)
            && self
                .inline_quantities
                .equals(&other.inline_quantities, converter)
    }
}

/// A section holding steps
#[derive(Debug, Default, Serialize, Deserialize, PartialEq, Clone)]
#[cfg_attr(feature = "ts", derive(Tsify))]
pub struct Section {
    /// Name of the section
    pub name: Option<String>,
    /// Content inside
    pub content: Vec<Content>,
}

impl Section {
    pub(crate) fn new(name: Option<String>) -> Section {
        Self {
            name,
            content: Vec::new(),
        }
    }

    /// Check if the section is empty
    ///
    /// A section is empty when it has no name and no content.
    pub fn is_empty(&self) -> bool {
        self.name.is_none() && self.content.is_empty()
    }
}

/// Each type of content inside a section
#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
#[cfg_attr(feature = "ts", derive(Tsify))]
#[serde(tag = "type", content = "value", rename_all = "camelCase")]
pub enum Content {
    /// A step
    Step(Step),
    /// A paragraph of just text, no instructions
    Text(String),
}

impl Content {
    /// Checks if the content is a regular step
    pub fn is_step(&self) -> bool {
        matches!(self, Self::Step(_))
    }

    /// Checks if the content is a text paragraph
    pub fn is_text(&self) -> bool {
        matches!(self, Self::Text(_))
    }

    /// Get's the inner step
    ///
    /// # Panics
    /// If the content is [`Content::Text`]
    pub fn unwrap_step(&self) -> &Step {
        match self {
            Content::Step(s) => s,
            Content::Text(_) => panic!("content is text"),
        }
    }

    /// Get's the inner step
    ///
    /// # Panics
    /// If the content is [`Content::Step`]
    pub fn unwrap_text(&self) -> &str {
        match self {
            Content::Step(_) => panic!("content is step"),
            Content::Text(t) => t.as_str(),
        }
    }
}

/// A step holding step [`Item`]s
#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
#[cfg_attr(feature = "ts", derive(Tsify))]
#[non_exhaustive]
pub struct Step {
    /// [`Item`]s inside
    pub items: Vec<Item>,

    /// Step number
    ///
    /// The step numbers start at 1 in each section and increase with non
    /// text step.
    pub number: u32,
}

/// A step item
///
/// Except for [`Item::Text`], the value is the index where the item is located
/// in it's corresponding [`Vec`] in the [`Recipe`].
#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
#[cfg_attr(feature = "ts", derive(Tsify))]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Item {
    /// Just plain text
    Text {
        value: String,
    },
    Ingredient {
        index: usize,
    },
    Cookware {
        index: usize,
    },
    Timer {
        index: usize,
    },
    InlineQuantity {
        index: usize,
    },
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[cfg_attr(feature = "ts", derive(Tsify))]
pub struct RecipeReference(RelativePathBuf);

impl PartialEq for RecipeReference {
    fn eq(&self, other: &Self) -> bool {
        self.0.normalize() == other.0.normalize()
    }
}

impl RecipeReference {
    fn prefix(path: RelativePathBuf) -> RelativePathBuf {
        if path.starts_with("./") || path.starts_with("../") {
            path
        } else {
            RelativePathBuf::from("./").join(&path)
        }
    }
    fn normalize(path: RelativePathBuf) -> RelativePathBuf {
        Self::prefix(path.normalize())
    }
    pub fn from(path: &str) -> Result<Self, ReferenceError> {
        match RelativePathBuf::from(path) {
            rp if rp.file_name().is_some() => Ok(RecipeReference(Self::normalize(rp))),
            _ => Err(ReferenceError::MissingName(path.to_string())),
        }
    }

    pub fn name(&self) -> &str {
        self.0
            .file_name()
            .expect("Reference must have a name! This should not be possible")
    }

    pub(crate) fn relative_to(&self, other: &RecipeReference) -> RecipeReference {
        let base = other
            .0
            .parent()
            .unwrap_or_else(|| relative_path::RelativePath::new(""));
        RecipeReference(Self::normalize(base.join(&self.0)))
    }
}

impl std::string::ToString for RecipeReference {
    fn to_string(&self) -> String {
        return self.0.to_string();
    }
}

/// A recipe ingredient
#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
#[cfg_attr(feature = "ts", derive(Tsify))]
#[cfg_attr(feature = "ts", tsify(into_wasm_abi, from_wasm_abi))]
pub struct Ingredient {
    /// Name
    ///
    /// This can have the form of a path if the ingredient references a recipe.
    pub name: String,
    /// Alias
    pub alias: Option<String>,
    /// Quantity
    pub quantity: Option<Quantity>,
    /// Note
    pub note: Option<String>,
    /// Recipe reference
    pub reference: Option<RecipeReference>,
    /// How the ingrient is related to others
    pub relation: IngredientRelation,
    #[cfg_attr(feature = "ts", serde(skip))]
    pub(crate) modifiers: Modifiers,

    // The source for the recipe
    pub source: Option<RecipeReference>,
}

impl Ingredient {
    /// Compares two ingredients using unit-aware quantity comparison.
    ///
    /// Unlike [`PartialEq`], the quantity is compared with [`Quantity::equals`],
    /// so `500 g` equals `0.5 kg`.
    ///
    /// [`source`](Self::source) is not part of the comparison: it records which
    /// file the ingredient came from, not what it is.
    pub fn equals(&self, other: &Self, converter: &Converter) -> bool {
        SemanticEq::equals(self, other, converter)
    }

    /// Gets the name the ingredient should be displayed with
    pub fn display_name(&self) -> Cow<'_, str> {
        let mut name = Cow::from(&self.name);
        if self.modifiers.contains(Modifiers::RECIPE) {
            if let Some(recipe_name) = std::path::Path::new(&self.name)
                .file_stem()
                .and_then(|s| s.to_str())
            {
                name = recipe_name.into();
            }
        }
        self.alias.as_ref().map(Cow::from).unwrap_or(name)
    }

    /// Access the ingredient modifiers
    pub fn modifiers(&self) -> Modifiers {
        self.modifiers
    }

    /// Groups all quantities from itself and it's references (if any).
    /// ```
    /// # use cooklang::{CooklangParser, Extensions, Converter, Value, Quantity};
    /// let parser = CooklangParser::new(Extensions::all(), Converter::bundled());
    /// let recipe = parser
    ///                 .parse("@flour{1000%g} @&flour{200%g} @&flour{1%bag}")
    ///                 .into_output()
    ///                 .unwrap();
    ///
    /// let flour = &recipe.ingredients[0];
    /// assert_eq!(flour.name, "flour");
    ///
    /// let grouped_flour = flour.group_quantities(
    ///                         &recipe.ingredients,
    ///                         parser.converter()
    ///                     );
    /// assert_eq!(grouped_flour.to_string(), "1.2 kg, 1 bag");
    /// assert_eq!(
    ///     grouped_flour.into_vec(),
    ///     vec![
    ///         Quantity::new(
    ///             Value::from(1.2),
    ///             Some("kg".to_string())  // Unit fit to kilograms
    ///         ),
    ///         Quantity::new(
    ///             Value::from(1.0),
    ///             Some("bag".to_string()) // Can't add this unit to kg
    ///         ),
    ///     ]
    /// );
    /// ```
    pub fn group_quantities(
        &self,
        all_ingredients: &[Self],
        converter: &Converter,
    ) -> GroupedQuantity {
        let mut grouped = GroupedQuantity::default();
        for q in self.all_quantities(all_ingredients) {
            grouped.add(q, converter);
        }
        let _ = grouped.fit(converter);
        grouped
    }

    /// Gets an iterator over all quantities of this ingredient and its references.
    pub fn all_quantities<'a>(
        &'a self,
        all_ingredients: &'a [Self],
    ) -> impl Iterator<Item = &'a Quantity> {
        std::iter::once(self.quantity.as_ref())
            .chain(
                self.relation
                    .referenced_from()
                    .iter()
                    .copied()
                    .map(|i| all_ingredients[i].quantity.as_ref()),
            )
            .flatten()
    }
}

impl SemanticEq for Ingredient {
    fn equals(&self, other: &Self, converter: &Converter) -> bool {
        self.name == other.name
            && self.alias == other.alias
            && self.note == other.note
            && self.relation == other.relation
            && self.reference == other.reference
            && self.modifiers == other.modifiers
            && self.quantity.equals(&other.quantity, converter)
    }
}

/// A recipe cookware item
#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
#[cfg_attr(feature = "ts", derive(Tsify))]
#[cfg_attr(feature = "ts", tsify(into_wasm_abi, from_wasm_abi))]
pub struct Cookware {
    /// Name
    pub name: String,
    /// Alias
    pub alias: Option<String>,
    /// Amount needed
    ///
    /// Note that this is a value, not a quantity, so it doesn't have units.
    pub quantity: Option<Quantity>,
    /// Note
    pub note: Option<String>,
    /// How the cookware is related to others
    pub relation: ComponentRelation,
    #[cfg_attr(feature = "ts", serde(skip))]
    pub(crate) modifiers: Modifiers,
}

impl Cookware {
    /// Compares two cookware items using unit-aware quantity comparison.
    ///
    /// Unlike [`PartialEq`], the quantity is compared with [`Quantity::equals`].
    pub fn equals(&self, other: &Self, converter: &Converter) -> bool {
        SemanticEq::equals(self, other, converter)
    }

    /// Gets the name the cookware item should be displayed with
    pub fn display_name(&self) -> &str {
        self.alias.as_ref().unwrap_or(&self.name)
    }

    /// Access the cookware modifiers
    pub fn modifiers(&self) -> Modifiers {
        self.modifiers
    }

    /// Same as [`Ingredient::group_quantities`] but for [`Cookware`]
    pub fn group_quantities(
        &self,
        all_cookware: &[Self],
        converter: &Converter,
    ) -> GroupedQuantity {
        let mut g = GroupedQuantity::empty();
        for q in self.all_quantities(all_cookware) {
            g.add(q, converter);
        }
        let _ = g.fit(converter);
        g
    }

    /// Gets an iterator over all quantities of this ingredient and its references.
    pub fn all_quantities<'a>(
        &'a self,
        all_cookware: &'a [Self],
    ) -> impl Iterator<Item = &'a Quantity> {
        std::iter::once(self.quantity.as_ref())
            .chain(
                self.relation
                    .referenced_from()
                    .iter()
                    .copied()
                    .map(|i| all_cookware[i].quantity.as_ref()),
            )
            .flatten()
    }
}

impl SemanticEq for Cookware {
    fn equals(&self, other: &Self, converter: &Converter) -> bool {
        self.name == other.name
            && self.alias == other.alias
            && self.note == other.note
            && self.relation == other.relation
            && self.modifiers == other.modifiers
            && self.quantity.equals(&other.quantity, converter)
    }
}

/// Relation between components
#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
#[cfg_attr(feature = "ts", derive(Tsify))]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum ComponentRelation {
    /// The component is a definition
    Definition {
        /// List of indices of other components of the same kind referencing this
        /// one
        referenced_from: Vec<usize>,
        /// True if the definition was in a step
        ///
        /// This is only false for components defined in components mode.
        defined_in_step: bool,
    },
    /// The component is a reference
    Reference {
        /// Index of the definition component
        references_to: usize,
    },
}

impl ComponentRelation {
    /// Gets a list of the components referencing this one.
    ///
    /// Returns a list of indices to the corresponding vec in [`Recipe`].
    pub fn referenced_from(&self) -> &[usize] {
        match self {
            ComponentRelation::Definition {
                referenced_from, ..
            } => referenced_from,
            ComponentRelation::Reference { .. } => &[],
        }
    }

    /// Get the index the relations references to
    pub fn references_to(&self) -> Option<usize> {
        match self {
            ComponentRelation::Definition { .. } => None,
            ComponentRelation::Reference { references_to } => Some(*references_to),
        }
    }

    /// Check if the relation is a reference
    pub fn is_reference(&self) -> bool {
        matches!(self, ComponentRelation::Reference { .. })
    }

    /// Check if the relation is a definition
    pub fn is_definition(&self) -> bool {
        matches!(self, ComponentRelation::Definition { .. })
    }

    /// Checks if the definition was in a step
    ///
    /// Returns None for references
    pub fn is_defined_in_step(&self) -> Option<bool> {
        match self {
            ComponentRelation::Definition {
                defined_in_step, ..
            } => Some(*defined_in_step),
            ComponentRelation::Reference { .. } => None,
        }
    }
}

/// Same as [`ComponentRelation`] but with the ability to reference steps and
/// sections apart from other ingredients.
#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
#[cfg_attr(feature = "ts", derive(Tsify))]
pub struct IngredientRelation {
    relation: ComponentRelation,
    reference_target: Option<IngredientReferenceTarget>,
}

/// Target an ingredient reference references to
///
/// This is obtained from [`IngredientRelation::references_to`]
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Hash, Clone, Copy)]
#[cfg_attr(feature = "ts", derive(Tsify))]
#[serde(rename_all = "camelCase")]
pub enum IngredientReferenceTarget {
    /// Ingredient definition
    Ingredient,
    /// Step in the current section
    Step,
    /// Section in the current recipe
    Section,
}

impl IngredientRelation {
    pub(crate) fn definition(referenced_from: Vec<usize>, defined_in_step: bool) -> Self {
        Self {
            relation: ComponentRelation::Definition {
                referenced_from,
                defined_in_step,
            },
            reference_target: None,
        }
    }

    pub(crate) fn reference(
        references_to: usize,
        reference_target: IngredientReferenceTarget,
    ) -> Self {
        Self {
            relation: ComponentRelation::Reference { references_to },
            reference_target: Some(reference_target),
        }
    }

    /// Gets a list of the components referencing this one.
    ///
    /// Returns a list of indices to the corresponding vec in [Recipe].
    pub fn referenced_from(&self) -> &[usize] {
        self.relation.referenced_from()
    }

    pub(crate) fn referenced_from_mut(&mut self) -> Option<&mut Vec<usize>> {
        match &mut self.relation {
            ComponentRelation::Definition {
                referenced_from, ..
            } => Some(referenced_from),
            ComponentRelation::Reference { .. } => None,
        }
    }

    /// Get the index the relation refrences to and the target
    ///
    /// The first element of the tuple is an index into:
    ///
    /// | Target | Where |
    /// |--------|-------|
    /// | [`Ingredient`] | [`Recipe::ingredients`] |
    /// | [`Step`] | [`Section::content`] in the same section this ingredient is. It's guaranteed that the content is a step. |
    /// | [`Section`] | [`Recipe::sections`] |
    ///
    /// [`Ingredient`]: IngredientReferenceTarget::Ingredient
    /// [`Step`]: IngredientReferenceTarget::Step
    /// [`Section`]: IngredientReferenceTarget::Section
    ///
    /// If the [`INTERMEDIATE_PREPARATIONS`](crate::Extensions::INTERMEDIATE_PREPARATIONS)
    /// extension is disabled, the target will always be
    /// [`IngredientReferenceTarget::Ingredient`].
    pub fn references_to(&self) -> Option<(usize, IngredientReferenceTarget)> {
        self.relation
            .references_to()
            .map(|index| (index, self.reference_target.unwrap()))
    }

    /// Checks if the relation is a regular reference to an ingredient
    pub fn is_regular_reference(&self) -> bool {
        use IngredientReferenceTarget as Target;
        self.references_to()
            .is_some_and(|(_, target)| target == Target::Ingredient)
    }

    /// Checks if the relation is an intermediate reference to a step or section
    pub fn is_intermediate_reference(&self) -> bool {
        use IngredientReferenceTarget as Target;
        self.references_to()
            .is_some_and(|(_, target)| matches!(target, Target::Step | Target::Section))
    }

    /// Check if the relation is a definition
    pub fn is_definition(&self) -> bool {
        self.relation.is_definition()
    }

    /// Checks if the definition was in a step
    ///
    /// Returns None for references
    pub fn is_defined_in_step(&self) -> Option<bool> {
        self.relation.is_defined_in_step()
    }
}

/// A recipe timer
///
/// If created from parsing, at least one of the fields is guaranteed to be
/// [`Some`].
#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
#[cfg_attr(feature = "ts", derive(Tsify))]
pub struct Timer {
    /// Name
    pub name: Option<String>,
    /// Time quantity
    ///
    /// If created from parsing the following applies:
    ///
    /// - If the [`ADVANCED_UNITS`](crate::Extensions::ADVANCED_UNITS) extension
    ///   is enabled, this is guaranteed to have a time unit and a non text value.
    ///
    /// - If the [`TIMER_REQUIRES_TIME`](crate::Extensions::TIMER_REQUIRES_TIME)
    ///   extension is enabled, this is guaranteed to be [`Some`].
    pub quantity: Option<Quantity>,
}

impl Timer {
    /// Compares two timers using unit-aware quantity comparison.
    ///
    /// Unlike [`PartialEq`], the quantity is compared with [`Quantity::equals`],
    /// so `1 h` equals `60 min`.
    pub fn equals(&self, other: &Self, converter: &Converter) -> bool {
        SemanticEq::equals(self, other, converter)
    }
}

impl SemanticEq for Timer {
    fn equals(&self, other: &Self, converter: &Converter) -> bool {
        self.name == other.name && self.quantity.equals(&other.quantity, converter)
    }
}

#[cfg(test)]
mod tests {
    use super::{Recipe, RecipeReference};
    use crate::{Converter, CooklangParser, Extensions};

    use indoc::indoc;
    use relative_path::RelativePathBuf;
    use std::sync::LazyLock;

    pub static PARSER: LazyLock<CooklangParser> =
        LazyLock::new(|| CooklangParser::new(Extensions::all(), Converter::default()));

    #[track_caller]
    fn recipe(s: &str) -> Recipe {
        let (recipe, _report) = PARSER.parse(s).into_result().unwrap();
        recipe
    }

    #[track_caller]
    fn rel(reference: &str, other: &str) -> String {
        rref(reference).relative_to(&rref(other)).to_string()
    }

    #[track_caller]
    fn rref(p: &str) -> RecipeReference {
        RecipeReference::from(p).unwrap()
    }

    #[test]
    fn recipe_compare() {
        let converter = Converter::default();
        let eq = |a: &str, b: &str| recipe(a).equals(&recipe(b), &converter);
        let ne = |a: &str, b: &str| !recipe(a).equals(&recipe(b), &converter);

        // identical recipes are equal
        assert!(eq(
            "@flour{200%g} and @butter{100%g}",
            "@flour{200%g} and @butter{100%g}",
        ));

        // unit conversion: 5 dl == 0.5 l, 5 min == 300 s
        assert!(eq(
            "Cook @water{5%dl} for ~{5%min}",
            "Cook @water{0.5%l} for ~{300%sec}",
        ));

        // different ingredient name is not equal
        assert!(ne("@flour{200%g}", "@sugar{200%g}"));

        // different quantity value is not equal
        assert!(ne("@flour{200%g}", "@flour{100%g}"));

        // different number of ingredients is not equal
        assert!(ne("@flour{200%g} and @butter{100%g}", "@flour{200%g}"));

        // timer unit conversion: 1 h == 60 min
        assert!(eq("Cook for ~{1%h}", "Cook for ~{60%min}"));

        // cookware without quantity is equal
        assert!(eq("Use #pan{}", "Use #pan{}"));

        // cookware without quantity is equal
        assert!(ne("Use #pan{small}", "Use #pan{big}"));

        // different metadata is not equal
        assert!(ne(
            indoc! {"
                >> title: Pasta
                @flour{200%g}
            "},
            indoc! {"
                >> title: Pizza
                @flour{200%g}
            "},
        ));

        // inline quantity: same value and unit is equal
        assert!(eq("Bake at 200ºC", "Bake at 200ºC"));

        // serves is an alias for servings
        assert!(eq(
            indoc! {"
                >> serves: 4
                @flour{200%g}
            "},
            indoc! {"
                >> servings: 4
                @flour{200%g}
            "},
        ));

        // different servings count is not equal
        assert!(ne(
            indoc! {"
                >> servings: 4
                @flour{200%g}
            "},
            indoc! {"
                >> servings: 8
                @flour{200%g}
            "},
        ));

        // yield is a quantity: 500%g == 0.5%kg (unit conversion)
        assert!(eq(
            indoc! {"
                >> yield: 500%g
                @flour{200%g}
            "},
            indoc! {"
                >> yield: 0.5%kg
                @flour{200%g}
            "},
        ));

        // yield with different amounts is not equal
        assert!(ne(
            indoc! {"
                >> yield: 500%g
                @flour{200%g}
            "},
            indoc! {"
                >> yield: 1000%g
                @flour{200%g}
            "},
        ));

        // yield and servings are distinct concepts
        assert!(ne(
            indoc! {"
                >> yield: 500%g
                @flour{200%g}
            "},
            indoc! {"
                >> servings: 4
                @flour{200%g}
            "},
        ));
    }
    #[test]
    fn sibling_in_same_directory() {
        assert_eq!(
            rel("./tomato", "recipes/pasta/spaghetti"),
            "./recipes/pasta/tomato"
        );
    }

    #[test]
    fn parent_directory_reference() {
        assert_eq!(
            rel("../sauces/tomato", "recipes/pasta/spaghetti"),
            "./recipes/sauces/tomato"
        );
    }

    #[test]
    fn multiple_parent_directory_references() {
        assert_eq!(
            rel("../../sauces/tomato", "recipes/italian/pasta/spaghetti"),
            "./recipes/sauces/tomato"
        );
    }

    #[test]
    fn other_at_root_has_no_parent() {
        // "spaghetti" has no directory component, so the base is the root.
        assert_eq!(rel("./sauce", "spaghetti"), "./sauce");
    }

    #[test]
    fn reference_without_dot_prefix_is_treated_as_relative() {
        // A bare relative path behaves the same as one with a "./" prefix.
        assert_eq!(
            rel("sauces/tomato", "recipes/pasta/spaghetti"),
            "./recipes/pasta/sauces/tomato"
        );
    }

    #[test]
    fn relative_to_self_resolves_to_own_path() {
        assert_eq!(rel("./spaghetti", "spaghetti"), "./spaghetti");
    }

    #[test]
    fn can_escape_above_the_other_reference_root() {
        // Going up more levels than "other" has just walks above the root;
        // relative_to does not clamp this.
        assert_eq!(rel("../tomato", "spaghetti"), "../tomato");
    }

    #[test]
    fn preserves_name_of_the_resolved_reference() {
        assert_eq!(
            rref("../sauces/tomato")
                .relative_to(&rref("./recipes/pasta/spaghetti"))
                .name(),
            "tomato"
        );
    }

    #[test]
    fn bare_path_is_prefixed_with_dot_slash() {
        assert_eq!(rref("spaghetti").to_string(), "./spaghetti");
        assert_eq!(rref("pasta/spaghetti").to_string(), "./pasta/spaghetti");
    }

    #[test]
    fn already_prefixed_path_is_left_unchanged() {
        assert_eq!(rref("./spaghetti").to_string(), "./spaghetti");
        assert_eq!(rref("../spaghetti").to_string(), "../spaghetti");
    }
}
