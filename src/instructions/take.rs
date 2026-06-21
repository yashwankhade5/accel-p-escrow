use pinocchio::{
    AccountView, ProgramResult,
    cpi::{Seed, Signer},
    error::ProgramError,
    sysvars::{Sysvar, rent::Rent},
};
use pinocchio_pubkey::derive_address;
use pinocchio_system::instructions::CreateAccount;
use pinocchio_associated_token_account::instructions::CreateIdempotent;
use pinocchio_token::instructions::{CloseAccount, TransferChecked};
use crate::state::Escrow;




pub fn process_take_instruction(accounts: &mut [AccountView], data: &[u8]) -> ProgramResult {
    let [
        maker,
        mint_a,
        mint_b,
        maker_ata_b,
        taker,
        taker_ata_a,
        taker_ata_b,
        escrow_account,
        escrow_ata,
        system_program,
        token_program,
        _associated_token_program @ ..,
    ] = accounts
    else {
        return Err(ProgramError::NotEnoughAccountKeys);
    };

    {
        let taker_ata_state = pinocchio_token::state::Account::from_account_view(taker_ata_b)?;
        if taker_ata_state.owner() != taker.address() {
            return Err(ProgramError::IllegalOwner);
        }
        if taker_ata_state.mint() != mint_b.address() {
            return Err(ProgramError::InvalidAccountData);
        }
    }

     CreateIdempotent {
        wallet: taker,
        funding_account: taker,
        mint: mint_a,
        account: taker_ata_a,
        system_program,
        token_program,
    }
    .invoke()?;

 {
        let taker_ata_a_state = pinocchio_token::state::Account::from_account_view(taker_ata_a)?;
        if taker_ata_a_state.owner() != taker.address() {
            return Err(ProgramError::InvalidAccountOwner);
        }
        if taker_ata_a_state.mint() != mint_a.address() {
            return Err(ProgramError::InvalidAccountData);
        }
    }

     CreateIdempotent {
        wallet: maker,
        funding_account: taker,
        mint: mint_b,
        account: maker_ata_b,
        system_program,
        token_program,
    }
    .invoke()?;


     {
        let maker_ata_b_state = pinocchio_token::state::Account::from_account_view(maker_ata_b)?;
        if maker_ata_b_state.owner() != maker.address() {
            return Err(ProgramError::InvalidAccountOwner);
        }
        if maker_ata_b_state.mint() != mint_b.address() {
            return Err(ProgramError::InvalidAccountData);
        }
    }


    let escrow_bump = {
        let escrow_state = Escrow::from_account_info(escrow_account)?;
        if escrow_state.maker().as_array() != maker.address().as_array() {
            return Err(ProgramError::InvalidAccountOwner);
        }

        escrow_state.bump
    };

   let seeds: [&[u8]; 3] = [b"escrow", maker.address().as_array(), &[escrow_bump]];
    let escrow_pda = derive_address(&seeds, None, crate::ID.as_array());

    if escrow_pda != *escrow_account.address().as_array() {
        return Err(ProgramError::InvalidAccountData);
    }

    let (amount_to_take, amount_to_give) = {
        let escrow_state = Escrow::from_account_info(escrow_account)?;
        (
            escrow_state.amount_to_receive(),
            escrow_state.amount_to_give(),
        )
    };

    let bump_bytes = [escrow_bump];
    let seeds = &[
        Seed::from(b"escrow"),
        Seed::from(maker.address().as_ref()),
        Seed::from(bump_bytes.as_ref()),
    ];

    let mint_b_state = pinocchio_token::state::Mint::from_account_view(mint_b)?;
    TransferChecked::new(
        taker_ata_b,
        mint_b,
        maker_ata_b,
        taker,
        amount_to_take,
        mint_b_state.decimals(),
    ).invoke()?;

    let mint_a_state = pinocchio_token::state::Mint::from_account_view(mint_a)?;

    TransferChecked::new(
        escrow_ata,
        mint_a,
        taker_ata_a,
        escrow_account,
        amount_to_give,
        mint_a_state.decimals(),
    ).invoke_signed(&[Signer::from(seeds)])?;

    CloseAccount::new(escrow_ata, maker, escrow_account).invoke_signed(&[Signer::from(seeds)])?;

    let escrow_lamports = escrow_account.lamports();
    maker.set_lamports(maker.lamports() + escrow_lamports);
    escrow_account.set_lamports(0);
    escrow_account.close()?;

    Ok(())
}
