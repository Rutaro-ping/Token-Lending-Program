pub(super) const MAX_DECIMALS: u32 = 18;
// Approach 1: Using const array

// power table for decimals, now support 0-18 decimals
pub(super) const DECIMALS_LOOKUP: [u64; 19] = [
    1,                   // 0 decimals
    10,                  // 1 decimal
    100,                 // 2 decimals
    1000,                // 3 decimals
    10000,               // 4 decimals
    100000,              // 5 decimals
    1000000,             // 6 decimals
    10000000,            // 7 decimals
    100000000,           // 8 decimals
    1000000000,          // 9 decimals
    10000000000,         // 10 decimals
    100000000000,        // 11 decimals
    1000000000000,       // 12 decimals
    10000000000000,      // 13 decimals
    100000000000000,     // 14 decimals
    1000000000000000,    // 15 decimals
    10000000000000000,   // 16 decimals
    100000000000000000,  // 17 decimals
    1000000000000000000, // 18 decimals
];
