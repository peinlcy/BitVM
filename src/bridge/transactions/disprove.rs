use bitcoin::{
    absolute, consensus, Amount, Network, PublicKey, ScriptBuf, TapSighashType, Transaction, TxOut,
    XOnlyPublicKey,
};
use musig2::{secp256k1::schnorr::Signature, PartialSignature, PubNonce, SecNonce};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::{
    super::{
        connectors::{connector::*, connector_5::Connector5, connector_c::ConnectorC},
        contexts::{base::BaseContext, operator::OperatorContext, verifier::VerifierContext},
        graphs::base::FEE_AMOUNT,
        scripts::*,
    },
    base::*,
    pre_signed::*,
    pre_signed_musig2::*,
    signing::push_taproot_leaf_script_and_control_block_to_witness,
};

#[derive(Serialize, Deserialize, Eq, PartialEq, Clone)]
pub struct DisproveTransaction {
    #[serde(with = "consensus::serde::With::<consensus::serde::Hex>")]
    tx: Transaction,
    #[serde(with = "consensus::serde::With::<consensus::serde::Hex>")]
    prev_outs: Vec<TxOut>,
    prev_scripts: Vec<ScriptBuf>,
    connector_5: Connector5,
    connector_c: ConnectorC,
    reward_output_amount: Amount,

    musig2_nonces: HashMap<usize, HashMap<PublicKey, PubNonce>>,
    musig2_nonce_signatures: HashMap<usize, HashMap<PublicKey, Signature>>,
    musig2_signatures: HashMap<usize, HashMap<PublicKey, PartialSignature>>,
}

impl PreSignedTransaction for DisproveTransaction {
    fn tx(&self) -> &Transaction { &self.tx }

    fn tx_mut(&mut self) -> &mut Transaction { &mut self.tx }

    fn prev_outs(&self) -> &Vec<TxOut> { &self.prev_outs }

    fn prev_scripts(&self) -> &Vec<ScriptBuf> { &self.prev_scripts }
}

impl PreSignedMusig2Transaction for DisproveTransaction {
    fn musig2_nonces(&self) -> &HashMap<usize, HashMap<PublicKey, PubNonce>> { &self.musig2_nonces }
    fn musig2_nonces_mut(&mut self) -> &mut HashMap<usize, HashMap<PublicKey, PubNonce>> {
        &mut self.musig2_nonces
    }
    fn musig2_nonce_signatures(&self) -> &HashMap<usize, HashMap<PublicKey, Signature>> {
        &self.musig2_nonce_signatures
    }
    fn musig2_nonce_signatures_mut(
        &mut self,
    ) -> &mut HashMap<usize, HashMap<PublicKey, Signature>> {
        &mut self.musig2_nonce_signatures
    }
    fn musig2_signatures(&self) -> &HashMap<usize, HashMap<PublicKey, PartialSignature>> {
        &self.musig2_signatures
    }
    fn musig2_signatures_mut(
        &mut self,
    ) -> &mut HashMap<usize, HashMap<PublicKey, PartialSignature>> {
        &mut self.musig2_signatures
    }
}

impl DisproveTransaction {
    pub fn new(
        context: &OperatorContext,
        input_0: Input,
        input_1: Input,
        script_index: u32,
    ) -> Self {
        Self::new_for_validation(
            context.network,
            &context.operator_taproot_public_key,
            &context.n_of_n_taproot_public_key,
            input_0,
            input_1,
            script_index,
        )
    }

    pub fn new_for_validation(
        network: Network,
        operator_taproot_public_key: &XOnlyPublicKey,
        n_of_n_taproot_public_key: &XOnlyPublicKey,
        input_0: Input,
        input_1: Input,
        script_index: u32,
    ) -> Self {
        let connector_5 = Connector5::new(network, &n_of_n_taproot_public_key);
        let connector_c = ConnectorC::new(network, &operator_taproot_public_key);

        let input_0_leaf = 1;
        let _input_0 = connector_5.generate_taproot_leaf_tx_in(input_0_leaf, &input_0);

        let input_1_leaf = script_index;
        let _input_1 = connector_c.generate_taproot_leaf_tx_in(input_1_leaf, &input_1);

        let total_output_amount = input_0.amount + input_1.amount - Amount::from_sat(FEE_AMOUNT);

        let _output_0 = TxOut {
            value: total_output_amount / 2,
            script_pubkey: generate_burn_script_address(network).script_pubkey(),
        };

        let reward_output_amount = total_output_amount - (total_output_amount / 2);
        let _output_1 = TxOut {
            value: reward_output_amount,
            script_pubkey: ScriptBuf::default(),
        };

        DisproveTransaction {
            tx: Transaction {
                version: bitcoin::transaction::Version(2),
                lock_time: absolute::LockTime::ZERO,
                input: vec![_input_0, _input_1],
                output: vec![_output_0, _output_1],
            },
            prev_outs: vec![
                TxOut {
                    value: input_0.amount,
                    script_pubkey: connector_5.generate_taproot_address().script_pubkey(),
                },
                TxOut {
                    value: input_1.amount,
                    script_pubkey: connector_c.generate_taproot_address().script_pubkey(),
                },
            ],
            prev_scripts: vec![
                connector_5.generate_taproot_leaf_script(input_0_leaf),
                connector_c.generate_taproot_leaf_script(input_1_leaf),
            ],
            connector_5,
            connector_c,
            reward_output_amount,
            musig2_nonces: HashMap::new(),
            musig2_nonce_signatures: HashMap::new(),
            musig2_signatures: HashMap::new(),
        }
    }

    fn sign_input_0(&mut self, context: &VerifierContext, secret_nonce: &SecNonce) {
        let input_index = 0;
        pre_sign_musig2_taproot_input(
            self,
            context,
            input_index,
            TapSighashType::Single,
            secret_nonce,
        );

        // TODO: Consider verifying the final signature against the n-of-n public key and the tx.
        if self.musig2_signatures[&input_index].len() == context.n_of_n_public_keys.len() {
            self.finalize_input_0(context);
        }
    }

    fn finalize_input_0(&mut self, context: &dyn BaseContext) {
        let input_index = 0;
        finalize_musig2_taproot_input(
            self,
            context,
            input_index,
            TapSighashType::Single,
            self.connector_5.generate_taproot_spend_info(),
        );
    }

    pub fn push_nonces(&mut self, context: &VerifierContext) -> HashMap<usize, SecNonce> {
        let mut secret_nonces = HashMap::new();

        let input_index = 0;
        let secret_nonce = push_nonce(self, context, input_index);
        secret_nonces.insert(input_index, secret_nonce);

        secret_nonces
    }

    pub fn pre_sign(
        &mut self,
        context: &VerifierContext,
        secret_nonces: &HashMap<usize, SecNonce>,
    ) {
        let input_index = 0;
        self.sign_input_0(context, &secret_nonces[&input_index]);
    }

    pub fn add_input_output(&mut self, input_script_index: u32, output_script_pubkey: ScriptBuf) {
        // Add output
        let output_index = 1;
        self.tx.output[output_index].script_pubkey = output_script_pubkey;

        let input_index = 1;

        // Push the unlocking witness
        let unlock_witness = self
            .connector_c
            .generate_taproot_leaf_script_witness(input_script_index);
        self.tx.input[input_index].witness.push(unlock_witness);

        // Push script + control block
        let script = self
            .connector_c
            .generate_taproot_leaf_script(input_script_index);
        let taproot_spend_info = self.connector_c.generate_taproot_spend_info();
        push_taproot_leaf_script_and_control_block_to_witness(
            &mut self.tx,
            input_index,
            &taproot_spend_info,
            &script,
        );
    }

    pub fn merge(&mut self, disprove: &DisproveTransaction) {
        merge_transactions(&mut self.tx, &disprove.tx);
        merge_musig2_nonces_and_signatures(self, disprove);
    }
}

impl BaseTransaction for DisproveTransaction {
    fn finalize(&self) -> Transaction {
        if self.tx.input.len() < 2 || self.tx.output.len() < 2 {
            panic!("Missing input or output. Call add_input_output before finalizing");
        }

        self.tx.clone()
    }
}