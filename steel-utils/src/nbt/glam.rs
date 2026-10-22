use glam::{Mat4, Quat, Vec3};
use simdnbt::owned::{NbtCompound, NbtList, NbtTag};
use simdnbt::{FromNbtTag, ToNbtTag};
use std::f32::consts;

use crate::nbt::NbtNumeric;
use simdnbt::borrow::NbtTag as BorrowedNbtTag;

fn numeric_f32_list(tag: BorrowedNbtTag, len: usize) -> Option<Vec<f32>> {
    if let Some(list) = tag.list() {
        return list
            .to_owned()
            .as_nbt_tags()
            .iter()
            .map(NbtNumeric::codec_f32)
            .take(len)
            .collect::<Option<Vec<_>>>()
            .filter(|v| v.len() >= len);
    }

    tag.byte_array()
        .map(|v| v.iter().map(|&v| f32::from(v as i8)).collect::<Vec<_>>())
        .or_else(|| {
            tag.int_array()
                .map(|v| v.into_iter().map(|v| v as f32).collect::<Vec<_>>())
        })
        .or_else(|| {
            tag.long_array()
                .map(|v| v.into_iter().map(|v| v as f32).collect::<Vec<_>>())
        })
        .filter(|v| v.len() >= len)
        .map(|mut values| {
            values.truncate(len);
            values
        })
}

/// Converts a [`Vec3`] to its NBT representation (recreates `ExtraCodecs.VECTOR3F`).
#[must_use]
pub fn vec3_to_nbt_tag(vec3: Vec3) -> NbtTag {
    NbtList::Float(vec![vec3.x, vec3.y, vec3.z]).into()
}

/// Tries to convert a [`Vec3`] from its NBT representation (recreates `ExtraCodecs.VECTOR3F`).
#[must_use]
pub fn vec3_from_nbt_tag(tag: BorrowedNbtTag) -> Option<Vec3> {
    numeric_f32_list(tag, 3).map(|floats| Vec3::new(floats[0], floats[1], floats[2]))
}

/// A rotation storing an angle and axis (in 3 components).
///
/// This is only used to provide a more accurate codec implementation
/// for quaternions (`Quat`s).
#[derive(Debug, Clone, Copy, PartialEq)]
struct AxisAngle4f {
    pub angle: f32,
    pub axis: Vec3,
}

impl ToNbtTag for AxisAngle4f {
    fn to_nbt_tag(self) -> NbtTag {
        let mut compound = NbtCompound::new();
        compound.insert("angle", self.angle);
        compound.insert("axis", vec3_to_nbt_tag(self.axis));
        NbtTag::Compound(compound)
    }
}

impl FromNbtTag for AxisAngle4f {
    fn from_nbt_tag(tag: BorrowedNbtTag) -> Option<Self> {
        let compound = tag.compound()?;
        Some(Self {
            angle: compound.get("angle")?.codec_f32()?,
            axis: vec3_from_nbt_tag(compound.get("axis")?)?,
        })
    }
}

impl From<AxisAngle4f> for Quat {
    fn from(value: AxisAngle4f) -> Self {
        // AxisAngle4f wraps its angle before Quaternionf converts it. Its axis is
        // deliberately not normalized, including when supplied through NBT.
        let tau = consts::TAU;
        let angle = if value.angle < 0.0 {
            tau + value.angle % tau
        } else {
            value.angle
        } % tau;
        let half_angle = angle * 0.5;
        let sin = f64::from(half_angle).sin() as f32;
        // JOML's cosFromSin reconstructs the magnitude from the rounded sine.
        let mut cos = f64::from(1.0 - sin * sin).sqrt() as f32;
        let shifted = half_angle + consts::FRAC_PI_2;
        let mut wrapped = shifted - (shifted / tau) as i32 as f32 * tau;
        if wrapped < 0.0 {
            wrapped += tau;
        }
        if wrapped >= consts::PI {
            cos = -cos;
        }
        Self::from_xyzw(
            value.axis.x * sin,
            value.axis.y * sin,
            value.axis.z * sin,
            cos,
        )
    }
}

/// Convert a [`Quat`] to its NBT representation (recreates `ExtraCodecs.QUATERNIONF`).
#[must_use]
pub fn quat_to_nbt_tag(quat: Quat) -> NbtTag {
    NbtList::Float(vec![quat.x, quat.y, quat.z, quat.w]).into()
}

/// Tries to convert a [`Quat`] from its NBT representation (recreates `ExtraCodecs.QUATERNIONF`).
#[must_use]
pub fn quat_from_nbt_tag(tag: BorrowedNbtTag) -> Option<Quat> {
    // One of the two: 4 floats or AxisAngle4f
    if let Some(l) = numeric_f32_list(tag, 4) {
        return Some(Quat::from_xyzw(l[0], l[1], l[2], l[3]).normalize());
    }
    Some(AxisAngle4f::from_nbt_tag(tag)?.into())
}

/// Tries to convert a [`Mat4`] from its NBT representation (recreates `ExtraCodecs.MATRIX4F`).
#[must_use]
pub fn mat4_to_nbt_tag(mat: Mat4) -> NbtTag {
    let elements = mat.transpose().to_cols_array();
    NbtList::Float(elements.to_vec()).into()
}

/// Tries to convert a [`Mat4`] from its NBT representation (recreates `ExtraCodecs.MATRIX4F`).
#[must_use]
pub fn mat4_from_nbt_tag(tag: BorrowedNbtTag) -> Option<Mat4> {
    let list = numeric_f32_list(tag, 16)?;
    let floats = list.as_array()?;
    Some(Mat4::from_cols_array(floats).transpose())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn with_tag(tag: NbtTag, check: impl FnOnce(BorrowedNbtTag<'_, '_>)) {
        let mut compound = NbtCompound::new();
        compound.insert("value", tag);
        let mut bytes = Vec::new();
        compound.write(&mut bytes);
        let owned = simdnbt::borrow::read_compound(&mut Cursor::new(bytes.as_slice())).unwrap();
        let borrowed = simdnbt::borrow::NbtCompound::from(&owned);
        check(borrowed.get("value").unwrap());
    }

    #[test]
    fn glam_nbt_accepts_numeric_arrays_and_oversized_partial_values() {
        with_tag(NbtTag::IntArray(vec![1, 2, 3, 4]), |tag| {
            assert_eq!(vec3_from_nbt_tag(tag), Some(Vec3::new(1.0, 2.0, 3.0)));
        });
        let values: Vec<i32> = (0..17).collect();
        with_tag(NbtTag::IntArray(values), |tag| {
            let matrix = mat4_from_nbt_tag(tag).unwrap();
            assert_eq!(matrix.row(0).to_array(), [0.0, 1.0, 2.0, 3.0]);
            assert_eq!(matrix.row(3).to_array(), [12.0, 13.0, 14.0, 15.0]);
        });
        with_tag(NbtTag::IntArray(vec![1, 2]), |tag| {
            assert!(vec3_from_nbt_tag(tag).is_none());
        });
    }

    #[test]
    fn glam_nbt_axis_angle_wraps_negative_angles_and_coerces_numbers() {
        let mut axis_angle = NbtCompound::new();
        axis_angle.insert("angle", -1_i32);
        axis_angle.insert("axis", NbtTag::IntArray(vec![0, 0, 2]));
        with_tag(axis_angle.into(), |tag| {
            let rotation = quat_from_nbt_tag(tag).unwrap();
            assert!(rotation.w < 0.0);
            assert!(rotation.z > 0.9);
            // The non-unit axis must not be normalized by the codec.
            assert!(rotation.length() > 1.0);
        });
    }

    #[test]
    fn glam_nbt_matrix_round_trip_preserves_row_major_layout() {
        let matrix = Mat4::from_translation(Vec3::new(1.0, 2.0, 3.0));
        with_tag(mat4_to_nbt_tag(matrix), |tag| {
            assert_eq!(mat4_from_nbt_tag(tag), Some(matrix));
        });
    }
}
