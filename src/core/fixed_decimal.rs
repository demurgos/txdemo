use num::{CheckedAdd, CheckedMul, CheckedSub, Integer};
use serde::{Deserialize, Serialize, Serializer};
use std::fmt::{Debug, Display, Formatter};
use std::marker::PhantomData;
use std::str::FromStr;
use thiserror::Error;

/// A decimal number with a fixed precision of `10 ** (-FRACTIONAL_DIGITS)` backed by a `T`.
///
/// This type is intended to represent currency amounts. As such it rejects situations leading to
/// rounding and forces you to use checked arithmetic.
///
/// Internally this is represented as an integral number of fractions (`= 10 ** (-PRECISION)`).
/// You can control the underlying integral type.
///
/// The underlying type is bound by [num::Integer], it gives you the flexibility to use your
/// own custom integer type (e.g. [num::BigInt]. Note however that the type must at least support
/// representing the numbers in `0..=10`, it means that using `NonZero*` types is not possible yet.
///
/// # Examples
///
/// | Type                   | Min       | Max                                     | Precision
/// | `FixedDecimal<u64, 2>` | 0         | 184467440737095516.15 (`(2**64-1)/100`) | 0.01
/// | `FixedDecimal<u64, 1>` | 0         | 1844674407370955161.5                   | 0.1
/// | `FixedDecimal<u64, 0>` | 0         | 18446744073709551615                    | 1
/// | `FixedDecimal<i16, 0>` | -32768    | 32767                                   | 1
/// | `FixedDecimal<i16, 4>` | -3.2768   | 3.2767                                  | 1e-4
/// | `FixedDecimal<i16, 5>` | -0.32768  | 0.32767                                 | 1e-5
/// | `FixedDecimal<i16, 6>` | -0.032768 | 0.032767                                | 1e-6
#[derive(Eq, PartialEq, Ord, PartialOrd, Hash, Copy, Clone, Default)]
pub struct FixedDecimal<T: Integer, const FRACTIONAL_DIGITS: u8>(T);

impl<T: Integer, const FRACTIONAL_DIGITS: u8> FixedDecimal<T, FRACTIONAL_DIGITS> {
    /// Build a fixed decimal value from a number of fractions only.
    pub fn from_fractions(fractions: T) -> Self {
        Self(fractions)
    }

    pub fn fractions(&self) -> &T {
        &self.0
    }
}

impl<T, const FRACTIONAL_DIGITS: u8> FixedDecimal<T, FRACTIONAL_DIGITS>
where
    T: Integer + CheckedMul + From<u8>,
{
    /// Returns the number of fractions in a unit.
    ///
    /// This corresponds to `10 ** PRECISION`.
    /// Returns `None` if a unit cannot be represented: e.g. `FixedPoint<u8, 3>`
    /// can only represent values in `[0, 0.255]`.
    pub fn fractions_per_unit() -> Option<T> {
        let ten: T = 10.into();
        let mut fractions = T::one();
        for _ in 0..FRACTIONAL_DIGITS {
            fractions = fractions.checked_mul(&ten)?;
        }
        Some(fractions)
    }
}

impl<T: Integer + Debug, const FRACTIONAL_DIGITS: u8> Debug for FixedDecimal<T, FRACTIONAL_DIGITS> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "FixedDecimal({:?}e-{})", self.0, FRACTIONAL_DIGITS)
    }
}

impl<T: Integer + CheckedAdd, const FRACTIONAL_DIGITS: u8> FixedDecimal<T, FRACTIONAL_DIGITS> {
    pub fn checked_add(&self, v: &Self) -> Option<Self> {
        num::CheckedAdd::checked_add(&self.0, &v.0).map(Self::from_fractions)
    }
}

impl<T: Integer + CheckedSub, const FRACTIONAL_DIGITS: u8> FixedDecimal<T, FRACTIONAL_DIGITS> {
    pub fn checked_sub(&self, v: &Self) -> Option<Self> {
        num::CheckedSub::checked_sub(&self.0, &v.0).map(Self::from_fractions)
    }
}

impl<T, const FRACTIONAL_DIGITS: u8> Display for FixedDecimal<T, FRACTIONAL_DIGITS>
where
    T: Integer + Display + CheckedMul + CheckedSub + From<u8> + Clone,
{
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        if FRACTIONAL_DIGITS == 0 {
            return self.0.fmt(f);
        }

        let (sign, abs) = if self.0 < T::zero() {
            ("-", T::zero().checked_sub(&self.0))
        } else {
            ("", Some(self.0.clone()))
        };

        match abs {
            Some(abs) => {
                let (int, frac) = match Self::fractions_per_unit() {
                    None => (T::zero(), abs),
                    Some(fpu) => abs.div_rem(&fpu),
                };

                write!(
                    f,
                    "{}{}.{:0>precision$}",
                    sign,
                    int,
                    frac,
                    precision = usize::from(FRACTIONAL_DIGITS)
                )
            }
            None => {
                // Failed to compute absolute value (e.g. -128i8)
                // This can only happen when dealing with negative numbers.
                // Fall back to manual printing...
                let zero = T::zero();
                let one = T::one();
                let ten = T::from(10);
                let mut n = self.0.clone();
                // Digits from right to left
                let mut digits: Vec<T> = Vec::new();
                while n != zero {
                    let (q, r) = n.div_mod_floor(&ten);
                    let digit = if q < zero && r != zero {
                        // Fix values after div_floor on negative dividend
                        n = q.add(one.clone());
                        ten.checked_sub(&r).expect("cannot represent digit")
                    } else {
                        n = q;
                        r
                    };
                    digits.push(digit);
                }
                write!(f, "{}", sign)?;
                let digit_count = usize::min(digits.len(), FRACTIONAL_DIGITS.into());
                let separator_index = digit_count - usize::from(FRACTIONAL_DIGITS);
                for i in 0..=digit_count {
                    if let Some(d) = digits.iter().rev().nth(i) {
                        write!(f, "{}", d)?;
                    } else {
                        write!(f, "0")?;
                    }
                    if i == separator_index {
                        write!(f, ".")?;
                    }
                }
                Ok(())
            }
        }
    }
}

impl<T, const FRACTIONAL_DIGITS: u8> Serialize for FixedDecimal<T, FRACTIONAL_DIGITS>
where
    T: Integer + Display + CheckedMul + CheckedSub + From<u8> + Clone,
{
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.to_string().as_str())
    }
}

impl<'de, T, const FRACTIONAL_DIGITS: u8> Deserialize<'de> for FixedDecimal<T, FRACTIONAL_DIGITS>
where
    T: Integer + CheckedAdd + CheckedMul + CheckedSub + From<u8>,
{
    fn deserialize<D: ::serde::Deserializer<'de>>(
        deserializer: D,
    ) -> ::std::result::Result<Self, D::Error> {
        struct SerdeVisitor<T, const FRACTIONAL_DIGITS: u8>(PhantomData<T>)
        where
            T: Integer + CheckedAdd + CheckedMul + CheckedSub + From<u8>;
        impl<'de, T, const FRACTIONAL_DIGITS: u8> ::serde::de::Visitor<'de>
            for SerdeVisitor<T, FRACTIONAL_DIGITS>
        where
            T: Integer + CheckedAdd + CheckedMul + CheckedSub + From<u8>,
        {
            type Value = FixedDecimal<T, FRACTIONAL_DIGITS>;

            fn expecting(&self, fmt: &mut ::std::fmt::Formatter) -> std::fmt::Result {
                fmt.write_str("a string representing a decimal id")
            }

            fn visit_str<E: ::serde::de::Error>(
                self,
                value: &str,
            ) -> ::std::result::Result<Self::Value, E> {
                value.parse().map_err(E::custom)
            }
        }

        deserializer.deserialize_str(SerdeVisitor(PhantomData))
    }
}

#[derive(Error, Debug, Eq, PartialEq)]
pub enum ParseFixedDecimalError {
    #[error("the magnitude of the number is too large")]
    TooLarge,
    #[error("the number of fractional digits in the input exceeds the supported precision")]
    TooMuchFractionalDigits,
    #[error("the input contains an invalid character")]
    InvalidChar,
    #[error("there are no digits in the input")]
    NoDigits,
}

impl<T: Integer + CheckedAdd + CheckedMul + CheckedSub + From<u8>, const FRACTIONAL_DIGITS: u8>
    FromStr for FixedDecimal<T, FRACTIONAL_DIGITS>
{
    type Err = ParseFixedDecimalError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut bytes = s.bytes();
        let mut sign = T::one();
        if let Some(neg_one) = T::neg_one() {
            let saved = bytes.clone();
            match bytes.next() {
                Some(b'-') => sign = neg_one,
                _ => bytes = saved,
            }
        }
        lex_digits(bytes, sign)
    }
}

// impl<T: Integer + Unsigned + CheckedAdd + CheckedMul + From<u8>, const FRACTIONAL_DIGITS: u8>
//     FromStr for FixedDecimal<T, FRACTIONAL_DIGITS>
// {
//     type Err = ParseFixedDecimalError;
//
//     fn from_str(s: &str) -> Result<Self, Self::Err> {
//         lex_digits(s.bytes(), T::one())
//     }
// }

/// `sign`: -1 or 1
fn lex_digits<T: Integer + CheckedAdd + CheckedMul + From<u8>, const FRACTIONAL_DIGITS: u8>(
    mut input: std::str::Bytes,
    signum: T,
) -> Result<FixedDecimal<T, FRACTIONAL_DIGITS>, ParseFixedDecimalError> {
    let mut fractions: T = T::zero();
    let mut has_digit: bool = false;
    let mut decimal_digits: Option<u16> = None;
    let ten: T = 10.into();
    loop {
        match input.next() {
            None => break,
            Some(c @ b'0'..=b'9') => {
                has_digit = true;
                if let Some(dd) = decimal_digits {
                    if dd == u16::from(FRACTIONAL_DIGITS) {
                        return Err(ParseFixedDecimalError::TooMuchFractionalDigits);
                    }
                    decimal_digits = Some(dd + 1);
                }
                let digit: u8 = c - b'0';
                let digit: T = digit.into();
                let digit = signum
                    .checked_mul(&digit)
                    .ok_or(ParseFixedDecimalError::TooLarge)?;
                fractions = fractions
                    .checked_mul(&ten)
                    .ok_or(ParseFixedDecimalError::TooLarge)?
                    .checked_add(&digit)
                    .ok_or(ParseFixedDecimalError::TooLarge)?;
            }
            Some(b'.') => {
                if decimal_digits.is_none() {
                    decimal_digits = Some(0);
                } else {
                    // Multiple decimal separators
                    return Err(ParseFixedDecimalError::InvalidChar);
                }
            }
            _ => return Err(ParseFixedDecimalError::InvalidChar),
        }
    }
    if !has_digit {
        return Err(ParseFixedDecimalError::NoDigits);
    }
    // Shift the current value to match the expected precision
    let mut decimal_digits = decimal_digits.unwrap_or(0);
    while decimal_digits < u16::from(FRACTIONAL_DIGITS) {
        decimal_digits += 1;
        fractions = fractions
            .checked_mul(&ten)
            .ok_or(ParseFixedDecimalError::TooLarge)?;
    }
    Ok(FixedDecimal::from_fractions(fractions))
}

/// Helper trait to detect if an integer type is signed or not by building `-1`
trait NegOne: Integer {
    /// If the type supports `-1`, return it; otherwise return `None`.
    fn neg_one() -> Option<Self>;
}

impl<T: Integer + CheckedSub> NegOne for T {
    fn neg_one() -> Option<Self> {
        let zero = T::zero();
        let one = T::one();
        zero.checked_sub(&one)
    }
}

#[cfg(test)]
mod test {
    use crate::core::fixed_decimal::{FixedDecimal, ParseFixedDecimalError};
    use std::str::FromStr;

    macro_rules! test_parse_i16_4 {
        ($($name:ident($input:literal, $expected:expr));+$(;)?) => {
            $(
                #[test]
                fn $name() {
                    let actual = FixedDecimal::<i16, 4>::from_str($input);
                    assert_eq!(actual, $expected);
                }
            )+
        };
    }

    test_parse_i16_4! {
        parse_0("0", Ok(FixedDecimal::from_fractions(0)));
        parse_1_0000("1.0000", Ok(FixedDecimal::from_fractions(10000)));
        parse_1_000("1.000", Ok(FixedDecimal::from_fractions(10000)));
        parse_1_00("1.00", Ok(FixedDecimal::from_fractions(10000)));
        parse_1_0("1.0", Ok(FixedDecimal::from_fractions(10000)));
        parse_1_("1.", Ok(FixedDecimal::from_fractions(10000)));
        parse_1("1", Ok(FixedDecimal::from_fractions(10000)));
        parse_3_2767("3.2767", Ok(FixedDecimal::from_fractions(32767)));
        parse_3_2768("3.2768", Err(ParseFixedDecimalError::TooLarge));
        parse_neg3_2767("-3.2767", Ok(FixedDecimal::from_fractions(-32767)));
        parse_neg3_2768("-3.2768", Ok(FixedDecimal::from_fractions(-32768)));
        parse_neg3_2769("-3.2769", Err(ParseFixedDecimalError::TooLarge));
        parse_1_00000("1.00000", Err(ParseFixedDecimalError::TooMuchFractionalDigits));
        parse_empty("", Err(ParseFixedDecimalError::NoDigits));
    }

    macro_rules! test_display_i16_4 {
        ($($name:ident($input:literal, $expected:literal));+$(;)?) => {
            $(
                #[test]
                fn $name() {
                    let actual = FixedDecimal::<i16, 4>::from_str($input).unwrap().to_string();
                    let actual = actual.as_str();
                    assert_eq!(actual, $expected);
                }
            )+
        };
    }

    test_display_i16_4! {
        display_0("0", "0.0000");
        display_1("1", "1.0000");
        display_0_1("0.1", "0.1000");
        display_0_01("0.01", "0.0100");
        display_0_001("0.001", "0.0010");
        display_0_0001("0.0001", "0.0001");
        display_neg0_0001("-0.0001", "-0.0001");
        display_neg1("-1", "-1.0000");
        display_3_2767("3.2767", "3.2767");
        display_neg3_2767("-3.2767", "-3.2767");
        display_neg3_2768("-3.2768", "-3.2768");
    }
}
