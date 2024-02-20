/**
 * This file was automatically generated by @cosmwasm/ts-codegen@0.35.7.
 * DO NOT MODIFY IT BY HAND. Instead, modify the source JSONSchema file,
 * and run the @cosmwasm/ts-codegen generate command to regenerate this file.
 */

import { CosmWasmClient } from "@cosmjs/cosmwasm-stargate";
import {
  SpendingResponse,
  SpendingsByAccountResponse,
  Uint64,
} from "./SpendLimit.types";
export interface SpendLimitReadOnlyInterface {
  contractAddress: string;
  spending: ({
    account,
    authenticatorId,
  }: {
    account: string;
    authenticatorId: Uint64;
  }) => Promise<SpendingResponse>;
  spendingsByAccount: ({
    account,
  }: {
    account: string;
  }) => Promise<SpendingsByAccountResponse>;
}
export class SpendLimitQueryClient implements SpendLimitReadOnlyInterface {
  client: CosmWasmClient;
  contractAddress: string;

  constructor(client: CosmWasmClient, contractAddress: string) {
    this.client = client;
    this.contractAddress = contractAddress;
    this.spending = this.spending.bind(this);
    this.spendingsByAccount = this.spendingsByAccount.bind(this);
  }

  spending = async ({
    account,
    authenticatorId,
  }: {
    account: string;
    authenticatorId: Uint64;
  }): Promise<SpendingResponse> => {
    return this.client.queryContractSmart(this.contractAddress, {
      spending: {
        account,
        authenticator_id: authenticatorId,
      },
    });
  };
  spendingsByAccount = async ({
    account,
  }: {
    account: string;
  }): Promise<SpendingsByAccountResponse> => {
    return this.client.queryContractSmart(this.contractAddress, {
      spendings_by_account: {
        account,
      },
    });
  };
}