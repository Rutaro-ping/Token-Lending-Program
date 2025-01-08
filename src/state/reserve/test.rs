#[cfg(test)]
mod tests {
    use {
        crate::{
            error::LendingError,
            math::{Decimal, Rate, TryDiv, TryMul, PERCENT_SCALER, WAD},
            state::{
                reserve::{
                    reserve_collateral::ReserveCollateral, reserve_liquidity::ReserveLiquidity,
                    CollateralExchangeRate, FeeCalculation, ReserveConfig, ReserveFees,
                },
                Reserve, SLOTS_PER_YEAR,
            },
        },
        proptest::prelude::*,
        std::cmp::Ordering,
    };

    const MAX_LIQUIDITY: u64 = u64::MAX / 5;

    prop_compose! {
         // Creates rates (min, opt, max) where 0 <= min <= opt <= max <= MAX
        fn borrow_rates()(optimal_rate in 0..=u8::MAX)(
            min_rate in 0..=optimal_rate,
            optimal_rate in Just(optimal_rate),
            max_rate in optimal_rate..=u8::MAX,
        ) -> (u8, u8, u8) {
            (min_rate, optimal_rate, max_rate)
        }
    }

    // Creates rates (threshold, ltv) where 2 <= threshold <= 100 and threshold <=
    // ltv <= 1,000%
    prop_compose! {
        fn unhealthy_rates()(threshold in 2..=100u8)(
            ltv_rate in threshold as u64..=1000u64,
            threshold in Just(threshold),
        ) -> (Decimal, u8) {
            (Decimal::from_scaled_val(ltv_rate as u128 * PERCENT_SCALER as u128), threshold)
        }
    }
    // Creates a range of reasonable token conversion rates
    prop_compose! {
        fn token_conversion_rate()(
            conversion_rate in 1..=u16::MAX,
            invert_conversion_rate: bool,
        ) -> Decimal {
            let conversion_rate = Decimal::from(conversion_rate as u64);
            if invert_conversion_rate {
                Decimal::one().try_div(conversion_rate).unwrap()
            } else {
                conversion_rate
            }
        }
    }
    // Creates a range of reasonable collateral exchange rates
    prop_compose! {
        fn collateral_exchange_rate_range()(percent in 1..=500u64) -> CollateralExchangeRate {
            CollateralExchangeRate(Rate::from_scaled_val(percent * PERCENT_SCALER))
        }
    }

    proptest! {
        #[test]
        fn current_borrow_rate(
            total_liquidity in 0..=MAX_LIQUIDITY,
            borrowed_percent in 0..=WAD,
            optimal_utilization_rate in 0..=100u8,
            (min_borrow_rate, optimal_borrow_rate, max_borrow_rate) in borrow_rates(),
        ) {
            let borrowed_amount_wads = Decimal::from(total_liquidity).try_mul(Rate::from_scaled_val(borrowed_percent))?;
            let reserve = Reserve {
                liquidity: ReserveLiquidity {
                    borrowed_amount_wads,
                    available_amount: total_liquidity - borrowed_amount_wads.try_round_u64()?,
                    ..ReserveLiquidity::default()
                },
                config: ReserveConfig { optimal_utilization_rate, min_borrow_rate, optimal_borrow_rate, max_borrow_rate, ..ReserveConfig::default() },
                ..Reserve::default()
            };

            let current_borrow_rate = reserve.current_borrow_rate()?;
            assert!(current_borrow_rate >= Rate::from_percent(min_borrow_rate));
            assert!(current_borrow_rate <= Rate::from_percent(max_borrow_rate));

            let optimal_borrow_rate = Rate::from_percent(optimal_borrow_rate);
            let current_rate = reserve.liquidity.utilization_rate()?;
            match current_rate.cmp(&Rate::from_percent(optimal_utilization_rate)) {
                Ordering::Less => {
                    if min_borrow_rate == reserve.config.optimal_borrow_rate {
                        assert_eq!(current_borrow_rate, optimal_borrow_rate);
                    } else {
                        assert!(current_borrow_rate < optimal_borrow_rate);
                    }
                }
                Ordering::Equal => assert!(current_borrow_rate == optimal_borrow_rate),
                Ordering::Greater => {
                    if max_borrow_rate == reserve.config.optimal_borrow_rate {
                        assert_eq!(current_borrow_rate, optimal_borrow_rate);
                    } else {
                        assert!(current_borrow_rate > optimal_borrow_rate);
                    }
                }
            }
        }

        #[test]
        fn current_utilization_rate(
            total_liquidity in 0..=MAX_LIQUIDITY,
            borrowed_percent in 0..=WAD,
        ) {
            let borrowed_amount_wads = Decimal::from(total_liquidity).try_mul(Rate::from_scaled_val(borrowed_percent))?;
            let liquidity = ReserveLiquidity {
                borrowed_amount_wads,
                available_amount: total_liquidity - borrowed_amount_wads.try_round_u64()?,
                ..ReserveLiquidity::default()
            };

            let current_rate = liquidity.utilization_rate()?;
            assert!(current_rate <= Rate::one());
        }

        #[test]
        fn collateral_exchange_rate(
            total_liquidity in 0..=MAX_LIQUIDITY,
            borrowed_percent in 0..=WAD,
            collateral_multiplier in 0..=(5*WAD),
            borrow_rate in 0..=u8::MAX,
        ) {
            let borrowed_liquidity_wads = Decimal::from(total_liquidity).try_mul(Rate::from_scaled_val(borrowed_percent))?;
            let available_liquidity = total_liquidity - borrowed_liquidity_wads.try_round_u64()?;
            let mint_total_supply = Decimal::from(total_liquidity).try_mul(Rate::from_scaled_val(collateral_multiplier))?.try_round_u64()?;

            let mut reserve = Reserve {
                collateral: ReserveCollateral {
                    mint_total_supply,
                    ..ReserveCollateral::default()
                },
                liquidity: ReserveLiquidity {
                    borrowed_amount_wads: borrowed_liquidity_wads,
                    available_amount: available_liquidity,
                    ..ReserveLiquidity::default()
                },
                config: ReserveConfig {
                    min_borrow_rate: borrow_rate,
                    optimal_borrow_rate: borrow_rate,
                    optimal_utilization_rate: 100,
                    ..ReserveConfig::default()
                },
                ..Reserve::default()
            };

            let exchange_rate = reserve.collateral_exchange_rate()?;
            assert!(exchange_rate.0.to_scaled_val() <= 5u128 * WAD as u128);

            // After interest accrual, total liquidity increases and collateral are worth more
            reserve.accrue_interest(1)?;

            let new_exchange_rate = reserve.collateral_exchange_rate()?;
            if borrow_rate > 0 && total_liquidity > 0 && borrowed_percent > 0 {
                assert!(new_exchange_rate.0 < exchange_rate.0);
            } else {
                assert_eq!(new_exchange_rate.0, exchange_rate.0);
            }
        }

        #[test]
        fn compound_interest(
            slots_elapsed in 0..=SLOTS_PER_YEAR,
            borrow_rate in 0..=u8::MAX,
        ) {
            let mut reserve = Reserve::default();
            let borrow_rate = Rate::from_percent(borrow_rate);

            // Simulate running for max 1000 years, assuming that interest is
            // compounded at least once a year
            for _ in 0..1000 {
                reserve.liquidity.compound_interest(borrow_rate, slots_elapsed)?;
                reserve.liquidity.cumulative_borrow_rate_wads.to_scaled_val()?;
            }
        }

        #[test]
        fn reserve_accrue_interest(
            slots_elapsed in 0..=SLOTS_PER_YEAR,
            borrowed_liquidity in 0..=u64::MAX,
            borrow_rate in 0..=u8::MAX,
        ) {
            let borrowed_amount_wads = Decimal::from(borrowed_liquidity);
            let mut reserve = Reserve {
                liquidity: ReserveLiquidity {
                    borrowed_amount_wads,
                    ..ReserveLiquidity::default()
                },
                config: ReserveConfig {
                    max_borrow_rate: borrow_rate,
                    ..ReserveConfig::default()
                },
                ..Reserve::default()
            };

            reserve.accrue_interest(slots_elapsed)?;

            if borrow_rate > 0 && slots_elapsed > 0 {
                assert!(reserve.liquidity.borrowed_amount_wads > borrowed_amount_wads);
            } else {
                assert!(reserve.liquidity.borrowed_amount_wads == borrowed_amount_wads);
            }
        }

        #[test]
        fn borrow_fee_calculation(
            borrow_fee_wad in 0..WAD, // at WAD, fee == borrow amount, which fails
            flash_loan_fee_wad in 0..WAD, // at WAD, fee == borrow amount, which fails
            host_fee_percentage in 0..=100u8,
            borrow_amount in 3..=u64::MAX, // start at 3 to ensure calculation success
                                           // 0, 1, and 2 are covered in the minimum tests
                                           // @FIXME: ^ no longer true
        ) {
            let fees = ReserveFees {
                borrow_fee_wad,
                flash_loan_fee_wad,
                host_fee_percentage,
            };
            let (total_fee, host_fee) = fees.calculate_borrow_fees(Decimal::from(borrow_amount), FeeCalculation::Exclusive)?;

            // The total fee can't be greater than the amount borrowed, as long
            // as amount borrowed is greater than 2.
            // At a borrow amount of 2, we can get a total fee of 2 if a host
            // fee is also specified.
            assert!(total_fee <= borrow_amount);

            // the host fee can't be greater than the total fee
            assert!(host_fee <= total_fee);

            // for all fee rates greater than 0, we must have some fee
            if borrow_fee_wad > 0 {
                assert!(total_fee > 0);
            }

            if host_fee_percentage == 100 {
                // if the host fee percentage is maxed at 100%, it should get all the fee
                assert_eq!(host_fee, total_fee);
            }

            // if there's a host fee and some borrow fee, host fee must be greater than 0
            if host_fee_percentage > 0 && borrow_fee_wad > 0 {
                assert!(host_fee > 0);
            } else {
                assert_eq!(host_fee, 0);
            }
        }

        #[test]
        fn flash_loan_fee_calculation(
            borrow_fee_wad in 0..WAD, // at WAD, fee == borrow amount, which fails
            flash_loan_fee_wad in 0..WAD, // at WAD, fee == borrow amount, which fails
            host_fee_percentage in 0..=100u8,
            borrow_amount in 3..=u64::MAX, // start at 3 to ensure calculation success
                                           // 0, 1, and 2 are covered in the minimum tests
                                           // @FIXME: ^ no longer true
        ) {
            let fees = ReserveFees {
                borrow_fee_wad,
                flash_loan_fee_wad,
                host_fee_percentage,
            };
            let (total_fee, host_fee) = fees.calculate_flash_loan_fees(Decimal::from(borrow_amount))?;

            // The total fee can't be greater than the amount borrowed, as long
            // as amount borrowed is greater than 2.
            // At a borrow amount of 2, we can get a total fee of 2 if a host
            // fee is also specified.
            assert!(total_fee <= borrow_amount);

            // the host fee can't be greater than the total fee
            assert!(host_fee <= total_fee);

            // for all fee rates greater than 0, we must have some fee
            if borrow_fee_wad > 0 {
                assert!(total_fee > 0);
            }

            if host_fee_percentage == 100 {
                // if the host fee percentage is maxed at 100%, it should get all the fee
                assert_eq!(host_fee, total_fee);
            }

            // if there's a host fee and some borrow fee, host fee must be greater than 0
            if host_fee_percentage > 0 && borrow_fee_wad > 0 {
                assert!(host_fee > 0);
            } else {
                assert_eq!(host_fee, 0);
            }
        }
    }

    #[test]
    fn borrow_fee_calculation_min_host() {
        let fees = ReserveFees {
            borrow_fee_wad: 10_000_000_000_000_000, // 1%
            flash_loan_fee_wad: 0,
            host_fee_percentage: 20,
        };
        // only 2 tokens borrowed, get error
        let err = fees
            .calculate_borrow_fees(Decimal::from(2u64), FeeCalculation::Exclusive)
            .unwrap_err();
        assert_eq!(err, LendingError::BorrowTooSmall.into()); // minimum of 3

        // only 1 token borrowed, get error
        let err = fees
            .calculate_borrow_fees(Decimal::one(), FeeCalculation::Exclusive)
            .unwrap_err();
        assert_eq!(err, LendingError::BorrowTooSmall.into());

        let (total_fee, host_fee) = fees
            .calculate_borrow_fees(Decimal::zero(), FeeCalculation::Exclusive)
            .unwrap();
        assert_eq!(total_fee, 0);
        assert_eq!(host_fee, 0);
    }
    #[test]
    fn borrow_fee_calculation_min_no_host() {
        let fees = ReserveFees {
            borrow_fee_wad: 10_000_000_000_000_000, // 1%
            flash_loan_fee_wad: 0,
            host_fee_percentage: 0,
        };
        let (total_fee, host_fee) = fees
            .calculate_borrow_fees(Decimal::from(2u64), FeeCalculation::Exclusive)
            .unwrap();
        assert_eq!(total_fee, 1);
        assert_eq!(host_fee, 0);
        let err = fees
            .calculate_borrow_fees(Decimal::one(), FeeCalculation::Exclusive)
            .unwrap_err();
        assert_eq!(err, LendingError::BorrowTooSmall.into()); // minimum of 2 tokens

        // 0 amount borrowed, 0 fee
        let (total_fee, host_fee) = fees
            .calculate_borrow_fees(Decimal::zero(), FeeCalculation::Exclusive)
            .unwrap();
        assert_eq!(total_fee, 0);
        assert_eq!(host_fee, 0);
    }
    #[test]
    fn borrow_fee_calculation_host() {
        let fees = ReserveFees {
            borrow_fee_wad: 10_000_000_000_000_000, // 1%
            flash_loan_fee_wad: 0,
            host_fee_percentage: 20,
        };

        let (total_fee, host_fee) = fees
            .calculate_borrow_fees(Decimal::from(1000u64), FeeCalculation::Exclusive)
            .unwrap();

        assert_eq!(total_fee, 10); // 1% of 1000
        assert_eq!(host_fee, 2); // 20% of 10
    }
    #[test]
    fn borrow_fee_calculation_no_host() {
        let fees = ReserveFees {
            borrow_fee_wad: 10_000_000_000_000_000, // 1%
            flash_loan_fee_wad: 0,
            host_fee_percentage: 0,
        };

        let (total_fee, host_fee) = fees
            .calculate_borrow_fees(Decimal::from(1000u64), FeeCalculation::Exclusive)
            .unwrap();

        assert_eq!(total_fee, 10); // 1% of 1000
        assert_eq!(host_fee, 0); // 0 host fee
    }
}
