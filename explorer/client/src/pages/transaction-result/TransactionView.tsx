// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
    getMoveCallTransaction,
    getPublishTransaction,
    getTransactionKindName,
    getTransactions,
    getTransactionSender,
    getTransferObjectTransaction,
    getMovePackageContent,
    getObjectId,
} from '@mysten/sui.js';
import cl from 'classnames';

import Longtext from '../../components/longtext/Longtext';
import SendReceiveView from './SendReceiveView';
import { type DataType } from './TransactionResultType';
import TxLinks from './TxLinks';
import TxModuleView from './TxModuleView';
import TxResultHeader from './TxResultHeader';

import type {
    CertifiedTransaction,
    TransactionKindName,
    ExecutionStatusType,
    SuiTransactionKind,
    SuiObjectRef,
} from '@mysten/sui.js';

import styles from './TransactionResult.module.css';

type TxDataProps = CertifiedTransaction & {
    status: ExecutionStatusType;
    timestamp_ms: number | null;
    gasFee: number;
    txError: string;
    mutated: SuiObjectRef[];
    created: SuiObjectRef[];
};

function generateMutatedCreated(
    tx: TxDataProps,
    txKindName?: TransactionKindName | undefined
) {
    return [
        ...(tx.mutated?.length
            ? [
                  {
                      label: 'Mutated',
                      links: tx.mutated.map((obj) => obj.objectId),
                  },
              ]
            : []),
        ...(tx.created?.length
            ? [
                  {
                      label: 'Created',
                      links: tx.created.map((obj) => obj.objectId),
                  },
              ]
            : []),
    ];
}

function formatByTransactionKind(
    kind: TransactionKindName | undefined,
    data: SuiTransactionKind,
    sender: string
) {
    switch (kind) {
        case 'TransferObject':
            const transfer = getTransferObjectTransaction(data)!;
            return {
                title: 'Transfer',
                sender: {
                    value: sender,
                    link: true,
                    category: 'addresses',
                },
                objectId: {
                    value: transfer.objectRef.objectId,
                    link: true,
                    category: 'objects',
                },
                recipient: {
                    value: transfer.recipient,
                    category: 'addresses',
                    link: true,
                },
            };
        case 'Call':
            const moveCall = getMoveCallTransaction(data)!;
            return {
                title: 'Call',
                sender: {
                    value: sender,
                    link: true,
                    category: 'addresses',
                },
                package: {
                    value: getObjectId(moveCall.package),
                    link: true,
                    category: 'objects',
                },
                module: {
                    value: moveCall.module,
                },
                arguments: {
                    value: moveCall.arguments,
                    list: true,
                },
            };
        case 'Publish':
            const publish = getPublishTransaction(data)!;
            return {
                title: 'publish',
                module: {
                    value: Object.entries(getMovePackageContent(publish)!),
                },
                ...(sender
                    ? {
                          sender: {
                              value: sender,
                              link: true,
                              category: 'addresses',
                          },
                      }
                    : {}),
            };

        default:
            return {};
    }
}

type TxItemView = {
    title: string;
    content: {
        label?: string | number | any;
        value: string | number;
        link?: boolean;
        monotypeClass?: boolean;
    }[];
};

function ItemView({ data }: { data: TxItemView }) {
    return (
        <div className={styles.itemView}>
            <div className={styles.itemviewtitle}>{data.title}</div>
            <div className={styles.itemviewcontent}>
                {data.content.map((item, index) => {
                    return (
                        <div
                            key={index}
                            className={cl(
                                styles.itemviewcontentitem,
                                !item.label && styles.singleitem
                            )}
                        >
                            {item.label && (
                                <div className={styles.itemviewcontentlabel}>
                                    {item.label}
                                </div>
                            )}
                            <div
                                className={cl(
                                    styles.itemviewcontentvalue,
                                    item.monotypeClass && styles.mono
                                )}
                            >
                                {item.link ? (
                                    <Longtext
                                        text={item.value as string}
                                        category="objects"
                                        isLink={true}
                                    />
                                ) : (
                                    item.value
                                )}
                            </div>
                        </div>
                    );
                })}
            </div>
        </div>
    );
}

function TransactionView({ txdata }: { txdata: DataType }) {
    const txdetails = getTransactions(txdata)[0];
    const txKindName = getTransactionKindName(txdetails);
    const sender = getTransactionSender(txdata);
    const recipient = getTransferObjectTransaction(txdetails);
    const txKindData = formatByTransactionKind(txKindName, txdetails, sender);

    const txHeaderData = {
        txId: txdata.txId,
        status: txdata.status,
        txKindName: txKindName,
    };

    const transactionSignatureData = {
        title: 'Transaction Signatures',
        content: [
            {
                label: 'Signature',
                value: txdata.txSignature,
                monotypeClass: true,
            },
        ],
    };

    const validatorSignatureData = {
        title: 'Validator Signatures',
        content: txdata.authSignInfo.signatures.map((validatorSign) => ({
            value: validatorSign,
            monotypeClass: true,
        })),
    };

    const createdMutateData = generateMutatedCreated(txdata, txKindName);

    const sendreceive = {
        sender: sender,
        ...(txdata.timestamp_ms
            ? {
                  timestamp_ms: txdata.timestamp_ms,
              }
            : {}),
        recipient: [...(recipient?.recipient ? [recipient.recipient] : [])],
    };
    const GasStorageFees = {
        title: 'Gas & Storage Fees',
        content: [
            {
                label: 'Gas Payment',
                value: txdata.data.gasPayment.objectId,
                link: true,
            },
            {
                label: 'Gas Fees',
                value: txdata.gasFee,
            },
            {
                label: 'Gas Budget',
                value: txdata.data.gasBudget,
            },
            //TODO: Add Storage Fees
        ],
    };
    const typearguments =
        txKindData?.arguments && txKindData?.arguments?.value
            ? {
                  title: 'Arguments' as string,
                  content: txKindData.arguments.value.map((arg) => ({
                      value: arg as string,
                      monotypeClass: true,
                  })),
              }
            : false;
    return (
        <div className={cl(styles.txdetailsbg)}>
            <TxResultHeader data={txHeaderData} />
            <div className={styles.txgridcomponent} id={txdata.txId}>
                {sender && (
                    <section
                        className={cl([styles.txcomponent, styles.txsender])}
                    >
                        <div className={styles.txaddress}>
                            <SendReceiveView data={sendreceive} />
                        </div>
                    </section>
                )}
                <section
                    className={cl([styles.txcomponent, styles.txgridcolspan2])}
                >
                    <div className={styles.txlinks}>
                        {createdMutateData.map((item, idx) => (
                            <TxLinks data={item} key={idx} />
                        ))}
                    </div>
                </section>

                {txKindData?.module?.value &&
                    Array.isArray(txKindData?.module?.value) && (
                        <section
                            className={cl([
                                styles.txcomponent,
                                styles.txgridcolspan3,
                            ])}
                        >
                            <h3 className={styles.txtitle}>Modules </h3>
                            <div className={styles.txmodule}>
                                {txKindData.module.value
                                    .slice(0, 3)
                                    .map((item, idx) => (
                                        <TxModuleView itm={item} key={idx} />
                                    ))}
                            </div>
                        </section>
                    )}
            </div>
            <div className={styles.txgridcomponent}>
                {typearguments && <ItemView data={typearguments} />}
                <ItemView data={GasStorageFees} />
                <ItemView data={transactionSignatureData} />
                <ItemView data={validatorSignatureData} />
            </div>
        </div>
    );
}

export default TransactionView;
