use std::fmt;
use solana_program::program_error::ProgramError;

#[derive(Copy, Clone)]
pub enum ElusivError {
    InvalidInstruction, // 0

    SenderIsNotSigner, // 1
    SenderIsNotWritable, // 2
    InvalidAmount, // 3
    InvalidProof, // 4
    CouldNotParseProof, // 5
    CouldNotProcessProof, // 6
    InvalidMerkleRoot, // 7

    InvalidStorageAccount, // 8
    InvalidStorageAccountSize, // 9
    CouldNotCreateMerkleTree, // 10

    NullifierAlreadyUsed, // 11
    NoRoomForNullifier, // 12

    CommitmentAlreadyUsed, // 13
    NoRoomForCommitment, // 14

    DidNotFinishHashing, // 15

    InvalidRecipient, // 16
}

impl From<ElusivError> for ProgramError {
    fn from(e: ElusivError) -> Self {
        ProgramError::Custom(e as u32)
    }
}

impl fmt::Display for ElusivError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::InvalidInstruction =>
                write!(f, "InvalidInstruction"),
            Self::SenderIsNotSigner =>
                write!(f, "SenderIsNotSigner"),
            Self::SenderIsNotWritable =>
                write!(f, "SenderIsNotWritable"),
            Self::InvalidAmount =>
                write!(f, "InvalidAmount"),
            Self::InvalidProof =>
                write!(f, "InvalidProof"),
            Self::CouldNotProcessProof =>
                write!(f, "CouldNotProcessProof"),
            Self::InvalidMerkleRoot =>
                write!(f, "InvalidMerkleRoot"),
            Self::InvalidStorageAccount =>
                write!(f, "InvalidStorageAccount"),
            Self::InvalidStorageAccountSize =>
                write!(f, "InvalidStorageAccountSize"),
            Self::CouldNotCreateMerkleTree =>
                write!(f, "CouldNotCreateMerkleTree"),
            Self::NullifierAlreadyUsed =>
                write!(f, "NullifierAlreadyUsed"),
            Self::NoRoomForNullifier =>
                write!(f, "NoRoomForNullifier"),
            Self::CommitmentAlreadyUsed =>
                write!(f, "CommitmentAlreadyUsed"),
            Self::NoRoomForCommitment =>
                write!(f, "NoRoomForCommitment"),
            Self::DidNotFinishHashing =>
                write!(f, "DidNotFinishHashing"),
            Self::InvalidRecipient =>
                write!(f, "InvalidRecipient"),
            Self::CouldNotParseProof =>
                write!(f, "CouldNotParseProof"),
        }
    }
}