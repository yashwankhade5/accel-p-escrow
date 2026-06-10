use pinocchio::{
    AccountView, ProgramResult,
    cpi::{Seed, Signer},
    error::ProgramError,
    sysvars::{Sysvar, rent::Rent},
};
use pinocchio_pubkey::derive_address;
use pinocchio_system::instructions::CreateAccount;

use crate::state::Escrow;




pub fn process_take_instruction(accounts: &mut [AccountView], data: &[u8]) -> ProgramResult {
    let [
        maker,
        taker,
        taker_ata_a,
        taker_ata_b,
        mint_a,
        mint_b,
        escrow_account,
        maker_ata_b,
        escrow_ata_a,
        system_program,
        token_program,
        _associated_token_program @ ..,
    ] = accounts
    else {
        return Err(ProgramError::NotEnoughAccountKeys);
    };

    {
        let maker_ata_state = pinocchio_token::state::Account::from_account_view(maker_ata_b)?;
        if maker_ata_state.owner() != maker.address() {
            return Err(ProgramError::IllegalOwner);
        }
        if maker_ata_state.mint() != mint_b.address() {
            return Err(ProgramError::InvalidAccountData);
        }
    }

    let bump = data[0];
    let seed = [b"escrow".as_ref(), maker.address().as_ref(), &[bump]];

    let escrow_account_pda = derive_address(&seed, None, &crate::ID.to_bytes());
    assert_eq!(escrow_account_pda, *escrow_account.address().as_array());

    let amount_to_receive = unsafe { *(data.as_ptr().add(1) as *const u64) };
    let amount_to_give = unsafe { *(data.as_ptr().add(9) as *const u64) };

    let bump_bytes = [bump];
    let signer_seeds = [
        Seed::from(b"escrow"),
        Seed::from(maker.address().as_array()),
        Seed::from(bump_bytes.as_ref()),
    ];
    let signer = Signer::from(&signer_seeds);

    if escrow_account.owned_by(&crate::ID) {
        return Err(ProgramError::AccountAlreadyInitialized);
    }

    CreateAccount {
        from: maker,
        to: escrow_account,
        lamports: Rent::get()?.try_minimum_balance(Escrow::LEN)?,
        space: Escrow::LEN as u64,
        owner: &crate::ID,
    }
    .invoke_signed(&[signer])?;

    let escrow_state = Escrow::from_account_info(escrow_account)?;
    escrow_state.set_maker(maker.address());
    escrow_state.set_mint_a(mint_a.address());
    escrow_state.set_mint_b(mint_b.address());
    escrow_state.set_amount_to_receive(amount_to_receive);
    escrow_state.set_amount_to_give(amount_to_give);
    escrow_state.bump = bump;

    pinocchio_associated_token_account::instructions::Create {
        funding_account: maker,
        account: escrow_ata,
        wallet: escrow_account,
        mint: mint_a,
        token_program,
        system_program,
    }
    .invoke()?;

    pinocchio_token::instructions::Transfer::new(maker_ata, escrow_ata, maker, amount_to_give)
        .invoke()?;

    Ok(())
}
