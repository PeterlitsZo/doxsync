import { Decimal } from "decimal.js";

// Output construction must not inherit a user's restricted exponent range.
// Arithmetic on returned values has enough precision for any 96-bit coefficient.
const OutputDecimal = Decimal.clone({ precision: 29, minE: -100, maxE: 100 });

export function decimalToString(value: unknown): string | undefined {
  if (!Decimal.isDecimal(value)) return undefined;
  // Bound the fixed-point representation before allocating it. Rust performs
  // the final exact coefficient check; no conversion is allowed to round.
  if (!value.isFinite() || value.decimalPlaces() > 28 || value.e > 28) {
    throw Object.assign(new Error("Decimal outside the supported coefficient or scale range"), {
      kind: "InvalidData",
    });
  }
  return value.toFixed();
}

export function decimalFromString(value: string): Decimal {
  return new OutputDecimal(value);
}
