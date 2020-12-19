// The entry file of your WebAssembly module.
import * as CL from "../../../../contract_as/assembly";
import {Error, ErrorCode} from "../../../../contract_as/assembly/error";
import {U512} from "../../../../contract_as/assembly/bignum";
import {URef} from "../../../../contract_as/assembly/uref";
import {RuntimeArgs} from "../../../../contract_as/assembly/runtime_args";
import {getMainPurse} from "../../../../contract_as/assembly/account";
import {transferFromPurseToPurse} from "../../../../contract_as/assembly/purse";

const ARG_PHASE = "phase";
const ARG_AMOUNT = "amount";
const POS_ACTION = "get_payment_purse";

function standardPayment(amount: U512): void {
  let proofOfStake = CL.getSystemContract(CL.SystemContract.ProofOfStake);

  let mainPurse = getMainPurse();

  let output = CL.callContract(proofOfStake, POS_ACTION, new RuntimeArgs());

  let paymentPurseResult = URef.fromBytes(output);
  if (paymentPurseResult.hasError()) {
    Error.fromErrorCode(ErrorCode.InvalidPurse).revert();
    return;
  }
  let paymentPurse = paymentPurseResult.value;

  let error = transferFromPurseToPurse(
    mainPurse,
    paymentPurse,
    amount,
  );
  if (error !== null) {
    error.revert();
    return;
  }
}

export function call(): void {
  const amountBytes = CL.getNamedArg(ARG_AMOUNT);
  let amountResult = U512.fromBytes(amountBytes);
  if (amountResult.hasError()) {
      Error.fromErrorCode(ErrorCode.InvalidArgument).revert();
      return;
  }
  let amount = amountResult.value;

  const phaseBytes = CL.getNamedArg(ARG_PHASE);
  if (phaseBytes.length != 1) {
    Error.fromErrorCode(ErrorCode.InvalidArgument).revert();
    return;
  }

  const phase = <CL.Phase>phaseBytes[0];

  const caller = CL.getPhase();
  assert(<u8>phase == <u8>caller);

  standardPayment(amount);
}
