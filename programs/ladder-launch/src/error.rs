use pinocchio::program_error::ProgramError;

#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LaunchError {
    InvalidAccountOwner = 0,
    InvalidPda,
    AlreadyInitialized,
    NotInitialized,
    MissingSigner,
    InvalidMint,
    MintOrder, // token mint must sort below the quote mint so it is token A
    InvalidVault,
    InvalidWhirlpool,
    InvalidBundle,
    BundleFull,
    ReserveEmpty,
    ZeroAmount,
    MathOverflow,
    NotSeatHolder,
    SeatTooYoung,
    ExitCapReached,
    InvalidSeat,
    InvalidTickArray,
    InvalidTokenAccount,
    PoolNotInitialized,
    FloorAlreadySeeded,
    InvalidProgram,
    InvalidArgs,
    UnsupportedQuoteMint,
}

impl From<LaunchError> for ProgramError {
    fn from(e: LaunchError) -> Self {
        ProgramError::Custom(e as u32)
    }
}
