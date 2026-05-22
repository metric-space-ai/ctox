import {
    BehaviorSubject,
    filter,
    firstValueFrom,
    map,
    Subject,
    Subscription
} from 'rxjs';
import { addRxPlugin } from '../../plugin.ts';
import { rxStorageInstanceToReplicationHandler } from '../../replication-protocol/index.ts';
import type {
    RxCollection,
    RxError,
    RxReplicationHandler,
    RxReplicationWriteToMasterRow,
    RxTypeError
} from '../../types/index.d.ts';
import {
    ensureNotFalsy,
    getFromMapOrThrow,
    randomToken
} from '../../plugins/utils/index.ts';
import { RxDBLeaderElectionPlugin } from '../leader-election/index.ts';
import { replicateRxCollection } from '../replication/index.ts';
import {
    isMasterInWebRTCReplication,
    sendMessageAndAwaitAnswer
} from './webrtc-helper.ts';
import type {
    WebRTCConnectionHandler,
    WebRTCPeerState,
    WebRTCReplicationCheckpoint,
    WebRTCResponse,
    RxWebRTCReplicationState,
    SyncOptionsWebRTC
} from './webrtc-types.ts';
import { newRxError } from '../../rx-error.ts';

const CTOX_RXDB_PROTOCOL = 'ctox-rxdb-protocol-v1';
const CTOX_RXDB_BROWSER_CAPABILITIES = [
    'ctox-rxdb-browser-v1',
    'ctox-file-chunks-v1',
    'ctox-replication-handshake-v1',
    'ctox-schema-hash-v1',
    'ctox-peer-session-v1',
    'ctox-checkpoint-epoch-v1'
];
const CTOX_RXDB_BROWSER_SESSION_ID = randomToken(16);

export async function replicateWebRTC<RxDocType, PeerType>(
    options: SyncOptionsWebRTC<RxDocType, PeerType>
): Promise<RxWebRTCReplicationPool<RxDocType, PeerType>> {
    const collection = options.collection;
    addRxPlugin(RxDBLeaderElectionPlugin);

    // fill defaults
    if (options.pull) {
        if (!options.pull.batchSize) {
            options.pull.batchSize = 20;
        }
    }
    if (options.push) {
        if (!options.push.batchSize) {
            options.push.batchSize = 20;
        }
    }

    if (collection.database.multiInstance) {
        await collection.database.waitForLeadership();
    }

    // used to easier debug stuff
    let requestCounter = 0;
    const requestFlag = randomToken(10);
    function getRequestId() {
        const count = requestCounter++;
        return collection.database.token + '|' + requestFlag + '|' + count;
    }

    const storageToken = await collection.database.storageToken;
    const pool = new RxWebRTCReplicationPool(
        collection,
        options,
        await options.connectionHandlerCreator(options)
    );


    pool.subs.push(
        pool.connectionHandler.error$.subscribe(err => pool.error$.next(err)),
        pool.connectionHandler.disconnect$.subscribe(peer => pool.removePeer(peer))
    );

    /**
     * Answer control handshake requests from remote peers.
     */
    pool.subs.push(
        pool.connectionHandler.message$.pipe(
            filter(data => data.message.method === 'token' || data.message.method === 'ctoxProtocol')
        ).subscribe(async data => {
            const result = data.message.method === 'ctoxProtocol'
                ? await ctoxProtocolPayload(collection)
                : storageToken;
            pool.connectionHandler.send(data.peer, {
                id: data.message.id,
                result
            });
        })
    );

    const connectSub = pool.connectionHandler.connect$
        .pipe(
            filter(() => !pool.canceled)
        )
        .subscribe(async (peer) => {
            if (options.isPeerValid) {
                const isValid = await options.isPeerValid(peer);
                if (!isValid) {
                    return;
                }
            }

            let remotePeerSessionId = 'no-session';
            try {
                const protocolResponse = await sendMessageAndAwaitAnswer(
                    pool.connectionHandler,
                    peer,
                    {
                        id: getRequestId(),
                        method: 'ctoxProtocol',
                        params: [await ctoxProtocolPayload(collection)]
                    }
                );
                await ensureCtoxProtocolCompatible(protocolResponse.result, collection);
                remotePeerSessionId = peerSessionIdentifier(protocolResponse.result);
                options.ctox?.onPeerProtocol?.({
                    collection: collection.name,
                    protocol: String(protocolResponse.result?.protocol || ''),
                    capabilities: Array.isArray(protocolResponse.result?.capabilities)
                        ? protocolResponse.result.capabilities.filter((capability: any) => typeof capability === 'string')
                        : [],
                    peerSession: remotePeerSessionId,
                    checkpoint: protocolResponse.result?.collection?.checkpoint || null
                });
            } catch (error: any) {
                pool.error$.next(newRxError('RC_WEBRTC_PROTOCOL', {
                    error
                }));
                return;
            }

            let peerToken: string;
            try {
                const tokenResponse = await sendMessageAndAwaitAnswer(
                    pool.connectionHandler,
                    peer,
                    {
                        id: getRequestId(),
                        method: 'token',
                        params: []
                    }
                );
                peerToken = tokenResponse.result;
            } catch (error: any) {
                /**
                 * If could not get the tokenResponse,
                 * just ignore that peer.
                 */
                pool.error$.next(newRxError('RC_WEBRTC_PEER', {
                    error
                }));
                return;
            }
            const isMaster = await isMasterInWebRTCReplication(collection.database.hashFunction, storageToken, peerToken);

            let replicationState: RxWebRTCReplicationState<RxDocType> | undefined;
            if (isMaster) {
                const masterHandler = pool.masterReplicationHandler;
                const masterChangeStreamSub = masterHandler.masterChangeStream$.subscribe(ev => {
                    const streamResponse: WebRTCResponse = {
                        id: 'masterChangeStream$',
                        result: ev
                    };
                    pool.connectionHandler.send(peer, streamResponse);
                });

                // clean up the subscription
                pool.subs.push(
                    masterChangeStreamSub,
                    pool.connectionHandler.disconnect$.pipe(
                        filter(p => p === peer)
                    ).subscribe(() => masterChangeStreamSub.unsubscribe())
                );

                const messageSub = pool.connectionHandler.message$
                    .pipe(
                        filter(data => data.peer === peer),
                        filter(data => data.message.method !== 'token'),
                        filter(data => data.message.method !== 'ctoxProtocol')
                    )
                    .subscribe(async (data) => {
                        const { peer: msgPeer, message } = data;
                        /**
                         * If it is not a function,
                         * it means that the client requested the masterChangeStream$
                         */
                        const method = (masterHandler as any)[message.method].bind(masterHandler);
                        const result = await (method as any)(...message.params);
                        const response: WebRTCResponse = {
                            id: message.id,
                            result
                        };
                        pool.connectionHandler.send(msgPeer, response);
                    });
                pool.subs.push(messageSub);
            } else {
                replicationState = replicateRxCollection({
                    replicationIdentifier: [collection.name, options.topic, peerToken, remotePeerSessionId].join('||'),
                    collection: collection,
                    autoStart: true,
                    deletedField: '_deleted',
                    live: true,
                    retryTime: options.retryTime,
                    waitForLeadership: false,
                    pull: options.pull ? Object.assign({}, options.pull, {
                        async handler(lastPulledCheckpoint: WebRTCReplicationCheckpoint | undefined) {
                            const answer = await sendMessageAndAwaitAnswer(
                                pool.connectionHandler,
                                peer,
                                {
                                    method: 'masterChangesSince',
                                    params: [
                                        lastPulledCheckpoint,
                                        ensureNotFalsy(options.pull).batchSize
                                    ],
                                    id: getRequestId()
                                }
                            );
                            return answer.result;
                        },
                        stream$: pool.connectionHandler.response$.pipe(
                            filter(m => m.response.id === 'masterChangeStream$'),
                            map(m => m.response.result)
                        )

                    }) : undefined,
                    push: options.push ? Object.assign({}, options.push, {
                        async handler(docs: RxReplicationWriteToMasterRow<RxDocType>[]) {
                            const answer = await sendMessageAndAwaitAnswer(
                                pool.connectionHandler,
                                peer,
                                {
                                    method: 'masterWrite',
                                    params: [docs],
                                    id: getRequestId()
                                }
                            );
                            return answer.result;
                        }
                    }) : undefined
                });
            }
            pool.addPeer(peer, replicationState);
        });
    pool.subs.push(connectSub);
    return pool;
}

async function ctoxProtocolPayload(collection: RxCollection<any, any, any, any>) {
    return {
        protocol: CTOX_RXDB_PROTOCOL,
        capabilities: CTOX_RXDB_BROWSER_CAPABILITIES,
        collection: {
            name: collection.name,
            schemaVersion: collection.schema.version,
            schemaHash: await collection.schema.hash,
            checkpoint: await ctoxCheckpointPayload(collection)
        },
        peerSession: {
            role: 'browser',
            sessionId: CTOX_RXDB_BROWSER_SESSION_ID
        }
    };
}

async function ctoxCheckpointPayload(collection: RxCollection<any, any, any, any>) {
    const schemaHash = await collection.schema.hash;
    const latest = await collection.find({
        selector: {},
        sort: [
            { _meta: 'desc' } as any,
            { [collection.schema.primaryPath]: 'desc' } as any
        ],
        limit: 1
    } as any).exec().catch(() => []);
    const doc = latest[0]?.toJSON?.() || null;
    const latestLwt = Number(doc?._meta?.lwt || 0);
    const latestId = doc ? String(doc[collection.schema.primaryPath] || '') : '';
    const latestIdHash = latestId ? await sha256Hex(latestId) : '';
    const epoch = await sha256Hex([
        'ctox-rxdb-js-indexeddb',
        collection.name,
        schemaHash,
        latestLwt,
        latestId
    ].join('\n'));
    return {
        source: 'ctox-rxdb-js-indexeddb',
        state: 'advertised',
        collection: collection.name,
        schemaHash,
        latestLwt,
        latestIdHash,
        epoch
    };
}

async function sha256Hex(input: string): Promise<string> {
    const bytes = new TextEncoder().encode(input);
    const digest = await crypto.subtle.digest('SHA-256', bytes);
    return [...new Uint8Array(digest)]
        .map(byte => byte.toString(16).padStart(2, '0'))
        .join('');
}

async function ensureCtoxProtocolCompatible(
    payload: any,
    collection: RxCollection<any, any, any, any>
) {
    const protocol = payload && typeof payload.protocol === 'string' ? payload.protocol : '';
    if (protocol !== CTOX_RXDB_PROTOCOL) {
        throw newRxError('RC_WEBRTC_PROTOCOL', {
            expected: CTOX_RXDB_PROTOCOL,
            actual: protocol || null
        });
    }
    const remoteCollection = payload && typeof payload === 'object' ? payload.collection : null;
    const expectedSchemaHash = await collection.schema.hash;
    if (!remoteCollection || typeof remoteCollection !== 'object') {
        throw newRxError('RC_WEBRTC_PROTOCOL', {
            expected: 'collection schema hash',
            actual: null
        });
    }
    if (remoteCollection.name !== collection.name) {
        throw newRxError('RC_WEBRTC_PROTOCOL', {
            expected: collection.name,
            actual: remoteCollection.name || null
        });
    }
    if (remoteCollection.schemaHash !== expectedSchemaHash) {
        throw newRxError('RC_WEBRTC_PROTOCOL', {
            expected: expectedSchemaHash,
            actual: remoteCollection.schemaHash || null,
            collection: collection.name
        });
    }
}

function peerSessionIdentifier(payload: any): string {
    const session = payload && typeof payload === 'object' ? payload.peerSession : null;
    const role = session && typeof session.role === 'string' ? session.role : 'unknown';
    const sessionId = session && typeof session.sessionId === 'string' ? session.sessionId : 'no-session';
    return [role, sessionId].join(':');
}


/**
 * Because the WebRTC replication runs between many instances,
 * we use a Pool instead of returning a single replication state.
 */
export class RxWebRTCReplicationPool<RxDocType, PeerType> {
    peerStates$: BehaviorSubject<Map<PeerType, WebRTCPeerState<RxDocType, PeerType>>> = new BehaviorSubject(new Map());
    canceled: boolean = false;
    masterReplicationHandler: RxReplicationHandler<RxDocType, WebRTCReplicationCheckpoint>;
    subs: Subscription[] = [];

    public error$ = new Subject<RxError | RxTypeError>();

    constructor(
        public readonly collection: RxCollection<RxDocType, any, any, any>,
        public readonly options: SyncOptionsWebRTC<RxDocType, PeerType>,
        public readonly connectionHandler: WebRTCConnectionHandler<PeerType>
    ) {
        this.collection.onClose.push(() => this.cancel());
        this.masterReplicationHandler = rxStorageInstanceToReplicationHandler(
            collection.storageInstance,
            collection.conflictHandler,
            collection.database.token,
        );
    }

    addPeer(
        peer: PeerType,
        // only if isMaster=false it has a replicationState
        replicationState?: RxWebRTCReplicationState<RxDocType>
    ) {
        const peerState: WebRTCPeerState<RxDocType, PeerType> = {
            peer,
            replicationState,
            subs: []
        };
        this.peerStates$.next(this.peerStates$.getValue().set(peer, peerState));
        if (replicationState) {
            peerState.subs.push(
                replicationState.error$.subscribe(ev => this.error$.next(ev))
            );
        }
    }
    removePeer(peer: PeerType) {
        const peerStates = this.peerStates$.getValue();
        const peerState = peerStates.get(peer);
        if (!peerState) {
            return;
        }
        peerStates.delete(peer);
        this.peerStates$.next(peerStates);
        peerState.subs.forEach(sub => sub.unsubscribe());
        if (peerState.replicationState) {
            peerState.replicationState.cancel();
        }
    }

    // often used in unit tests
    awaitFirstPeer() {
        return firstValueFrom(
            this.peerStates$.pipe(
                filter(peerStates => peerStates.size > 0)
            )
        );
    }

    public async awaitInitialReplication(): Promise<true> {
        const replicationStates = await this.awaitPeerReplicationStates();
        await Promise.all(replicationStates.map(replicationState => {
            if (typeof replicationState.awaitInitialReplication === 'function') {
                return replicationState.awaitInitialReplication();
            }
            if (typeof replicationState.awaitInSync === 'function') {
                return replicationState.awaitInSync();
            }
            return true;
        }));
        return true;
    }

    public async awaitInSync(): Promise<true> {
        const replicationStates = await this.awaitPeerReplicationStates();
        await Promise.all(replicationStates.map(replicationState => {
            if (typeof replicationState.awaitInSync === 'function') {
                return replicationState.awaitInSync();
            }
            if (typeof replicationState.awaitInitialReplication === 'function') {
                return replicationState.awaitInitialReplication();
            }
            return true;
        }));
        return true;
    }

    private async awaitPeerReplicationStates(): Promise<RxWebRTCReplicationState<RxDocType>[]> {
        const peerStates = await this.awaitFirstPeer();
        return Array.from(peerStates.values())
            .map(peerState => peerState.replicationState)
            .filter((replicationState): replicationState is RxWebRTCReplicationState<RxDocType> => !!replicationState);
    }

    public async cancel() {
        if (this.canceled) {
            return;
        }
        this.canceled = true;
        this.subs.forEach(sub => sub.unsubscribe());
        Array.from(this.peerStates$.getValue().keys()).forEach(peer => {
            this.removePeer(peer);
        });
        await this.connectionHandler.close();
    }
}

export * from './webrtc-helper.ts';
export * from './webrtc-types.ts';
// export * from './connection-handler-webtorrent';
// export * from './connection-handler-p2pcf';
export * from './connection-handler-simple-peer.ts';
