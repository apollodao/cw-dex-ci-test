//! Module containing implementations of calculations needed for swapping

use cosmwasm_std::{Decimal, StdError, StdResult, Uint128};
use cw_asset::Asset;
use cw_bigint::BigInt;

/// Returns square root of a BigInt
fn bigint_sqrt(input: BigInt) -> StdResult<BigInt> {
    if input < 0.into() {
        return Err(StdError::generic_err("Cannot calculate square root of negative number"));
    }

    let mut x = input.clone();
    let mut y = (&x + 1u128) / 2u128;
    while y < x {
        x = y;
        y = (&x + &input / &x) / 2u128;
    }
    Ok(x)
}

/// For a constant product pool, calculates how much of one asset we need to
/// swap to the other in order to have the same ratio of assets as the pool, so
/// that we can then provide liquidity and get the most amount of LP tokens.
///
/// Returns `Asset` containing the amount and info of the asset we need to swap.
pub fn calc_xyk_balancing_swap(
    assets: [Asset; 2],
    reserve1: Uint128,
    reserve2: Uint128,
    fee: Decimal,
) -> StdResult<Asset> {
    // Instead of trying to implement our own big decimal, we just use BigInt
    // and multiply and divide with this number before and after doing
    // calculations.
    let precision: BigInt = BigInt::from(1_000_000_000u128);

    // Make sure there is liquidity in the pool
    if reserve1.is_zero() || reserve2.is_zero() {
        return Err(StdError::generic_err("No liquidity in pool"));
    }

    // Get ratio of reserves and provided assets
    let reserve_ratio = Decimal::from_ratio(reserve1, reserve2);
    let asset_ratio = Decimal::from_ratio(assets[0].amount, assets[1].amount);

    // Check which asset to swap
    let (offer_amount, ask_amount, offer_asset_info, offer_reserve, ask_reserve) =
        if asset_ratio.gt(&reserve_ratio) {
            (
                BigInt::from(assets[0].amount.u128()) * &precision,
                BigInt::from(assets[1].amount.u128()) * &precision,
                &assets[0].info,
                BigInt::from(reserve1.u128()) * &precision,
                BigInt::from(reserve2.u128()) * &precision,
            )
        } else {
            (
                BigInt::from(assets[1].amount.u128()) * &precision,
                BigInt::from(assets[0].amount.u128()) * &precision,
                &assets[1].info,
                BigInt::from(reserve2.u128()) * &precision,
                BigInt::from(reserve1.u128()) * &precision,
            )
        };

    let fee = (BigInt::from(fee.atomics().u128()) * &precision) / BigInt::from(10u128.pow(18));

    // Calculate amount to swap by setting up quadratic equation
    let a = &ask_reserve + &ask_amount;
    let b = 2u128 * &offer_reserve * (&ask_reserve + &ask_amount)
        - ((&offer_reserve + &offer_amount) * &ask_reserve * &fee) / &precision;
    let c = &offer_reserve * (&offer_reserve * &ask_amount - &offer_amount * &ask_reserve);

    // Solve quadratic equation
    let discriminant = &b * &b - (4u128 * &a * &c);
    //  We know that for this equation, there is only one positive real solution
    let x = (bigint_sqrt(discriminant)? - b) / (2u128 * a);

    // Divide by precision to get final result and convert to Uint128
    let amount = x / precision;
    let amount: u128 = amount
        .try_into()
        .map_err(|_| StdError::generic_err("Could not convert from BigInt to u128"))?;

    let offer_asset = Asset {
        amount: Uint128::from(amount),
        info: offer_asset_info.clone(),
    };

    Ok(offer_asset)
}

#[cfg(test)]
mod test {
    use cosmwasm_std::{Decimal, Uint128};
    use cw_asset::{Asset, AssetInfo};
    use cw_bigint::BigInt;

    use crate::math::{bigint_sqrt, calc_xyk_balancing_swap};

    /// Calculate how much will be returned from a swap in a constant product pool
    fn constant_product_formula(
        offer_reserve: Uint128,
        ask_reserve: Uint128,
        offer_amount: Uint128,
        fee: Decimal,
    ) -> Uint128 {
        let cp = offer_reserve * ask_reserve;
        let return_amount: Uint128 = (Decimal::from_ratio(ask_reserve, 1u8)
            - Decimal::from_ratio(cp, offer_reserve + offer_amount))
            * Uint128::from(1u8);
        return_amount * (Decimal::one() - fee)
    }

    /// Assert that two Decimals are almost the same (diff smaller than one permille)
    fn assert_decimal_almost_eq(a: Decimal, b: Decimal) {
        let diff = if a > b {
            a - b
        } else {
            b - a
        };
        if diff > Decimal::permille(1) {
            println!("Failed assert decimal almost eq for a: {}, b: {}. diff: {}", a, b, diff);
            panic!();
        }
    }

    // Assert that the ratio of the users assets is the same as the pool after the swap
    fn assert_asset_ratios_same_after_swap(
        offer_reserve: Uint128,
        ask_reserve: Uint128,
        offer_balance: Uint128,
        ask_balance: Uint128,
        offer_amount: Uint128,
        fee: Decimal,
    ) {
        let receive_amount =
            constant_product_formula(offer_reserve, ask_reserve, offer_amount, fee);
        let asset_ratio_after_swap =
            Decimal::from_ratio(ask_balance + receive_amount, offer_balance - offer_amount);
        let reserve_ratio_after_swap =
            Decimal::from_ratio(ask_reserve - receive_amount, offer_reserve + offer_amount);
        assert_decimal_almost_eq(asset_ratio_after_swap, reserve_ratio_after_swap);
    }

    #[test]
    fn test_calc_xyk_balancing_swap() {
        let test_cases = vec![
            // Test 1: 1:1 ratio, double amount of asset 2
            // Should swap some of uosmo for uatom
            (
                [
                    Asset {
                        amount: Uint128::from(1_000_000u128),
                        info: AssetInfo::native("uatom".to_string()),
                    },
                    Asset {
                        amount: Uint128::from(2_000_000u128),
                        info: AssetInfo::native("uosmo".to_string()),
                    },
                ],
                Uint128::from(1_000_000_000_000u128),
                Uint128::from(1_000_000_000_000u128),
            ),
            // Test 2: 1:5 ratio, double amount of asset 2
            // Should swap some of uatom for uatom
            (
                [
                    Asset {
                        amount: Uint128::from(1_000_000u128),
                        info: AssetInfo::native("uatom".to_string()),
                    },
                    Asset {
                        amount: Uint128::from(2_000_000u128),
                        info: AssetInfo::native("uosmo".to_string()),
                    },
                ],
                Uint128::from(1_000_000_000_000u128),
                Uint128::from(5_000_000_000_000u128),
            ),
            // Test 3: 1:3 pool ratio, 1:1 ratio of assets, but a lot of assets compared to pool (high slipage)
            // Should swap some of uatom for uosmo
            (
                [
                    Asset {
                        amount: Uint128::from(1_000_000_000_000u128),
                        info: AssetInfo::native("uatom".to_string()),
                    },
                    Asset {
                        amount: Uint128::from(1_000_000_000_000u128),
                        info: AssetInfo::native("uosmo".to_string()),
                    },
                ],
                Uint128::from(1_000_000_000_000u128),
                Uint128::from(3_000_000_000_000u128),
            ),
            // Test 4: 1:2 pool ratio, 0:1 ratio of assets
            // Should swap some of uosmo for uatom
            (
                [
                    Asset {
                        amount: Uint128::from(0u128),
                        info: AssetInfo::native("uatom".to_string()),
                    },
                    Asset {
                        amount: Uint128::from(1_000_000_000_000u128),
                        info: AssetInfo::native("uosmo".to_string()),
                    },
                ],
                Uint128::from(1_000_000_000_000u128),
                Uint128::from(2_000_000_000_000u128),
            ),
            // Test 5: 1:1 pool ratio, 1:1 ratio of assets
            // Should not swap
            (
                [
                    Asset {
                        amount: Uint128::from(1_000_000_000_000u128),
                        info: AssetInfo::native("uatom".to_string()),
                    },
                    Asset {
                        amount: Uint128::from(1_000_000_000_000u128),
                        info: AssetInfo::native("uosmo".to_string()),
                    },
                ],
                Uint128::from(1_000_000_000_000u128),
                Uint128::from(1_000_000_000_000u128),
            ),
        ];

        // Same fee for all test cases
        let fee = Decimal::permille(3);

        // Run test cases
        for (assets, reserve1, reserve2) in test_cases {
            // Compare ratios to define offer asset
            let asset_ratio = Decimal::from_ratio(assets[0].amount, assets[1].amount);
            let reserve_ratio = Decimal::from_ratio(reserve1, reserve2);
            let (offer_asset, ask_asset, offer_reserve, ask_reserve) =
                if asset_ratio > reserve_ratio {
                    (assets[0].clone(), assets[1].clone(), reserve1, reserve2)
                } else {
                    (assets[1].clone(), assets[0].clone(), reserve2, reserve1)
                };

            // Calculate swap
            let swap_asset =
                calc_xyk_balancing_swap(assets.clone(), reserve1, reserve2, fee).unwrap();

            // If ratios are already almost the same, no swap should happen
            if asset_ratio == reserve_ratio {
                assert_eq!(swap_asset.amount, Uint128::zero());
            }

            // Assert that the correct asset is being offered
            assert_eq!(swap_asset.info, offer_asset.info);

            // Assert that the asset ratio and the pool ratio are the same after the swap
            assert_asset_ratios_same_after_swap(
                offer_reserve,
                ask_reserve,
                offer_asset.amount,
                ask_asset.amount,
                swap_asset.amount,
                fee,
            );
        }
    }

    #[test]
    fn test_bigint_sqrt() {
        // Test the sqrt algorithm
        let test_cases = vec![
            (0, 0),
            (1, 1),
            (2, 1),
            (3, 1),
            (4, 2),
            (28, 5),
            (29, 5),
            (34, 5),
            (36, 6),
            (37, 6),
            (57, 7),
            (58, 7),
            (66, 8),
            (67, 8),
            (69, 8),
            (982734928374982u128, 31348603),
            (u128::MAX, 18446744073709551615u128),
        ];
        for (input, expected) in test_cases {
            let input = BigInt::from(input);
            let expected = BigInt::from(expected);
            let result = bigint_sqrt(input).unwrap();
            assert_eq!(result, expected);
        }

        // Some larger than u128::MAX test cases
        let test_cases = vec![
            (BigInt::from(u128::MAX) * 2, BigInt::from(26087635650665564424u128)),
            (BigInt::from(u128::MAX) * 4, BigInt::from(36893488147419103231u128)),
            (BigInt::from(u128::MAX) * 100, BigInt::from(184467440737095516159u128)),
            (BigInt::from(u128::MAX) * 1000, BigInt::from(583337266871351588485u128)),
        ];
        for (input, expected) in test_cases {
            let result = bigint_sqrt(input).unwrap();
            assert_eq!(result, expected);
        }
    }
}
