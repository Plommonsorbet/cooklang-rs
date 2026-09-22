//! Support for recipe scaling

use crate::metadata::round_servings;
use crate::{convert::Converter, quantity::Value, Quantity, Recipe};
use thiserror::Error;

/// Error type for scaling operations
#[derive(Debug, Error, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "ts", derive(tsify::Tsify))]
pub enum ScaleError {
    /// The recipe has no valid numeric servings value
    #[error("Cannot scale recipe: servings metadata is not a valid number")]
    InvalidServings,

    /// The recipe has no valid yield metadata
    #[error("Cannot scale recipe: yield metadata is missing or invalid")]
    InvalidYield,

    /// The units don't match between target and current yield
    #[error("Cannot scale recipe: unit mismatch (expected {expected}, got {got})")]
    UnitMismatch { expected: String, got: String },
}

impl Recipe {
    /// Scale a recipe
    ///
    pub fn scale(&mut self, factor: f64, converter: &Converter) {
        let scale_quantity = |q: &mut Quantity| {
            if q.scalable {
                q.value.scale(factor);
                let _ = q.fit(converter);
            }
        };

        // Update metadata with new servings (only if numeric)
        if let Some(current_servings) = self.metadata.servings() {
            if let Some(base) = current_servings.as_number() {
                let new_servings = round_servings(base * factor);
                if let Some(servings_value) =
                    self.metadata.get_mut(crate::metadata::StdKey::Servings)
                {
                    // Preserve the original type (string or number)
                    match servings_value {
                        serde_yaml::Value::String(_) => {
                            *servings_value = serde_yaml::Value::String(new_servings.to_string());
                        }
                        _ => {
                            *servings_value =
                                serde_yaml::Value::Number(serde_yaml::Number::from(new_servings));
                        }
                    }
                }
            }
        }

        self.ingredients
            .iter_mut()
            .filter_map(|i| i.quantity.as_mut())
            .for_each(scale_quantity);
        self.cookware
            .iter_mut()
            .filter_map(|i| i.quantity.as_mut())
            .for_each(scale_quantity);
        self.timers
            .iter_mut()
            .filter_map(|i| i.quantity.as_mut())
            .for_each(scale_quantity);
    }

    /// Scale to a specific number of servings
    ///
    /// - `target` is the wanted number of servings.
    ///
    /// Returns an error if the recipe doesn't have a valid numeric servings value.
    pub fn scale_to_servings(
        &mut self,
        target: f64,
        converter: &Converter,
    ) -> Result<(), ScaleError> {
        let current_servings = self
            .metadata
            .servings()
            .ok_or(ScaleError::InvalidServings)?;

        let base = current_servings
            .as_number()
            .ok_or(ScaleError::InvalidServings)?;

        let factor = target as f64 / base as f64;
        self.scale(factor, converter);

        // Update servings metadata to the target value
        if let Some(servings_value) = self.metadata.get_mut(crate::metadata::StdKey::Servings) {
            // Preserve the original type (string or number)
            match servings_value {
                serde_yaml::Value::String(_) => {
                    *servings_value = serde_yaml::Value::String(target.to_string());
                }
                _ => {
                    *servings_value = serde_yaml::Value::Number(serde_yaml::Number::from(target));
                }
            }
        }
        Ok(())
    }

    /// Scale to a target value with optional unit
    ///
    /// This function intelligently chooses the appropriate scaling method:
    /// - If `target_unit` is `Some("servings")`, scales by servings
    /// - If `target_unit` is `Some(other_unit)`, scales by yield with that unit
    /// - If `target_unit` is `None`, applies direct scaling factor
    ///
    /// # Arguments
    /// - `target_value` - The target value (servings count, yield amount, or scaling factor)
    /// - `target_unit` - Optional unit ("servings" for servings-based, other for yield-based, None for direct factor)
    /// - `converter` - Unit converter for fitting quantities
    ///
    /// # Returns
    /// - `Ok(())` on successful scaling
    /// - `Err(ScaleError)` if scaling cannot be performed
    pub fn scale_to_target(
        &mut self,
        target_value: f64,
        target_unit: Option<&str>,
        converter: &Converter,
    ) -> Result<(), ScaleError> {
        match target_unit {
            Some("servings") | Some("serving") => self.scale_to_servings(target_value, converter),
            Some(unit) => {
                // Scale by yield with the specified unit
                self.scale_to_yield(target_value, unit, converter)
            }
            None => {
                // Direct scaling factor
                self.scale(target_value, converter);
                Ok(())
            }
        }
    }

    /// Scale to a specific yield amount with unit
    ///
    /// - `target_value` is the wanted yield amount
    /// - `target_unit` is the unit for the yield
    ///
    /// Returns an error if:
    /// - The recipe doesn't have yield metadata
    /// - The yield metadata is not in the correct format
    /// - The units don't match
    pub fn scale_to_yield(
        &mut self,
        target_value: f64,
        target_unit: &str,
        converter: &Converter,
    ) -> Result<(), ScaleError> {
        let current_qty = self
            .metadata
            .yield_quantity()
            .ok_or(ScaleError::InvalidYield)?;

        let current_value = match current_qty.value() {
            Value::Number(n) => n.value(),
            _ => return Err(ScaleError::InvalidYield),
        };

        let current_unit = current_qty.unit().unwrap_or("").to_string();

        if current_unit != target_unit {
            return Err(ScaleError::UnitMismatch {
                expected: target_unit.to_string(),
                got: current_unit,
            });
        }

        let factor = target_value / current_value;
        self.scale(factor, converter);

        if let Some(yield_meta) = self.metadata.get_mut(crate::metadata::StdKey::Yield) {
            *yield_meta = serde_yaml::Value::String(format!("{target_value}%{target_unit}"));
        }

        Ok(())
    }
}

impl Value {
    fn scale(&mut self, factor: f64) {
        match self {
            Value::Number(n) => {
                *n = (n.value() * factor).into();
            }
            Value::Range { start, end } => {
                *start = (start.value() * factor).into();
                *end = (end.value() * factor).into();
            }
            Value::Text(_) => {}
        }
    }
}
