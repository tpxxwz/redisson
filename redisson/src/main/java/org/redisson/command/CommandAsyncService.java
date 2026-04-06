/**
 * Copyright (c) 2013-2026 Nikita Koksharov
 * <p>
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 * <p>
 * http://www.apache.org/licenses/LICENSE-2.0
 * <p>
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */
package org.redisson.command;

import io.netty.buffer.ByteBuf;
import io.netty.util.ReferenceCountUtil;
import org.redisson.api.RFuture;
import org.redisson.api.options.ObjectParams;
import org.redisson.client.RedisClient;
import org.redisson.client.RedisException;
import org.redisson.client.codec.Codec;
import org.redisson.client.codec.StringCodec;
import org.redisson.client.protocol.RedisCommand;
import org.redisson.client.protocol.RedisCommands;
import org.redisson.config.DefaultCommandMapper;
import org.redisson.config.DelayStrategy;
import org.redisson.connection.ConnectionManager;
import org.redisson.connection.MasterSlaveEntry;
import org.redisson.connection.NodeSource;
import org.redisson.connection.ServiceManager;
import org.redisson.misc.CompletableFutureWrapper;
import org.slf4j.Logger;
import org.slf4j.LoggerFactory;

import java.util.*;
import java.util.concurrent.CompletableFuture;
import java.util.concurrent.CompletionException;
import java.util.concurrent.CompletionStage;
import java.util.concurrent.ExecutionException;
import java.util.concurrent.atomic.AtomicBoolean;
import java.util.function.Function;
import java.util.regex.Matcher;
import java.util.regex.Pattern;

/**
 *
 * @author Nikita Koksharov
 *
 */
public class CommandAsyncService implements CommandAsyncExecutor {

    final Codec codec;
    final ConnectionManager connectionManager;
    private final int retryAttempts;
    private final DelayStrategy retryDelay;
    private final int responseTimeout;
    private final boolean trackChanges;

    @Override
    public CommandAsyncExecutor copy(boolean trackChanges) {
        return new CommandAsyncService(this, trackChanges);
    }

    protected CommandAsyncService(CommandAsyncExecutor executor, boolean trackChanges) {
        CommandAsyncService service = (CommandAsyncService) executor;
        this.codec = service.codec;
        this.connectionManager = service.connectionManager;
        this.retryAttempts = service.retryAttempts;
        this.retryDelay = service.retryDelay;
        this.responseTimeout = service.responseTimeout;
        this.trackChanges = trackChanges;
    }

    @Override
    public CommandAsyncExecutor copy(ObjectParams objectParams) {
        return new CommandAsyncService(this, objectParams);
    }

    protected CommandAsyncService(CommandAsyncExecutor executor,
                                  ObjectParams objectParams) {
        CommandAsyncService service = (CommandAsyncService) executor;
        this.codec = service.codec;
        this.connectionManager = service.connectionManager;

        if (objectParams.getRetryAttempts() >= 0) {
            this.retryAttempts = objectParams.getRetryAttempts();
        } else {
            this.retryAttempts = connectionManager.getServiceManager().getConfig().getRetryAttempts();
        }
        if (objectParams.getRetryDelay() != null) {
            this.retryDelay = objectParams.getRetryDelay();
        } else {
            this.retryDelay = connectionManager.getServiceManager().getConfig().getRetryDelay();
        }
        if (objectParams.getTimeout() > 0) {
            this.responseTimeout = objectParams.getTimeout();
        } else {
            this.responseTimeout = connectionManager.getServiceManager().getConfig().getTimeout();
        }
        this.trackChanges = false;
    }

    protected CommandAsyncService(ConnectionManager connectionManager) {
        this.connectionManager = connectionManager;
        this.codec = connectionManager.getServiceManager().getCfg().getCodec();
        this.retryAttempts = connectionManager.getServiceManager().getConfig().getRetryAttempts();
        this.retryDelay = connectionManager.getServiceManager().getConfig().getRetryDelay();
        this.responseTimeout = connectionManager.getServiceManager().getConfig().getTimeout();
        this.trackChanges = false;
    }

    @Override
    public ConnectionManager getConnectionManager() {
        return connectionManager;
    }

    @Override
    public <V> V getNow(CompletableFuture<V> future) {
        try {
            return future.getNow(null);
        } catch (Exception e) {
            return null;
        }
    }

    @Override
    public <V> void transfer(CompletionStage<V> future1, CompletableFuture<V> future2) {
        future1.whenComplete((res, e) -> {
            if (e != null) {
                future2.completeExceptionally(e);
                return;
            }

            future2.complete(res);
        });
    }

    @Override
    public <V> V get(RFuture<V> future) {
        if (Thread.currentThread().getName().startsWith("redisson-netty")) {
            throw new IllegalStateException("Sync methods can't be invoked from async/rx/reactive listeners");
        }

        try {
            return future.toCompletableFuture().get();
        } catch (InterruptedException e) {
            future.cancel(true);
            Thread.currentThread().interrupt();
            throw new RedisException(e);
        } catch (ExecutionException e) {
            throw convertException(e);
        }
    }

    @Override
    public <V> V get(CompletableFuture<V> future) {
        if (Thread.currentThread().getName().startsWith("redisson-netty")) {
            throw new IllegalStateException("Sync methods can't be invoked from async/rx/reactive listeners");
        }

        try {
            return future.get();
        } catch (InterruptedException e) {
            future.cancel(true);
            Thread.currentThread().interrupt();
            throw new RedisException(e);
        } catch (ExecutionException e) {
            throw convertException(e);
        }
    }

    @Override
    public <V> V getInterrupted(CompletableFuture<V> future) throws InterruptedException {
        try {
            return future.get();
        } catch (InterruptedException e) {
            future.completeExceptionally(e);
            throw e;
        } catch (ExecutionException e) {
            throw convertException(e);
        }
    }

    protected <R> CompletableFuture<R> createPromise() {
        return new CompletableFuture<R>();
    }

    @Override
    public <T, R> RFuture<R> readAsync(RedisClient client, String name, Codec codec, RedisCommand<T> command, Object... params) {
        int slot = connectionManager.calcSlot(name);
        return async(true, new NodeSource(slot, client), codec, command, params, false, false);
    }

    public <T, R> RFuture<R> readAsync(RedisClient client, byte[] key, Codec codec, RedisCommand<T> command, Object... params) {
        int slot = connectionManager.calcSlot(key);
        return async(true, new NodeSource(slot, client), codec, command, params, false, false);
    }

    @Override
    public <T, R> RFuture<R> readAsync(RedisClient client, Codec codec, RedisCommand<T> command, Object... params) {
        return async(true, new NodeSource(client), codec, command, params, false, false);
    }

    private <R, T> void retryReadRandomAsync(Codec codec, RedisCommand<T> command, CompletableFuture<R> mainPromise,
                                             List<RedisClient> nodes, Object... params) {
        RedisClient client = nodes.remove(0);
        MasterSlaveEntry masterSlaveEntry = connectionManager.getEntry(client);
        RFuture<R> attemptPromise = async(true, new NodeSource(masterSlaveEntry, client), codec, command, params, false, false);
        attemptPromise.whenComplete((res, e) -> {
            if (e == null) {
                if (res == null) {
                    if (nodes.isEmpty()) {
                        mainPromise.complete(null);
                    } else {
                        retryReadRandomAsync(codec, command, mainPromise, nodes, params);
                    }
                } else {
                    mainPromise.complete(res);
                }
            } else {
                mainPromise.completeExceptionally(e);
            }
        });
    }

    public RedisException convertException(ExecutionException e) {
        if (e.getCause() instanceof RedisException) {
            return (RedisException) e.getCause();
        }
        return new RedisException("Unexpected exception while processing command", e.getCause());
    }

    private NodeSource getNodeSource(String key) {
        int slot = connectionManager.calcSlot(key);
        return new NodeSource(slot);
    }

    private NodeSource getNodeSource(byte[] key) {
        int slot = connectionManager.calcSlot(key);
        return new NodeSource(slot);
    }

    private NodeSource getNodeSource(ByteBuf key) {
        int slot = connectionManager.calcSlot(key);
        return new NodeSource(slot);
    }

    @Override
    public <T, R> RFuture<R> readAsync(String key, Codec codec, RedisCommand<T> command, Object... params) {
        NodeSource source = getNodeSource(key);
        return async(true, source, codec, command, params, false, false);
    }

    public <T, R> RFuture<R> readAsync(MasterSlaveEntry entry, Codec codec, RedisCommand<T> command, Object... params) {
        return async(true, new NodeSource(entry), codec, command, params, false, false);
    }

    @Override
    public <T, R> RFuture<R> writeAsync(MasterSlaveEntry entry, Codec codec, RedisCommand<T> command, Object... params) {
        return async(false, new NodeSource(entry), codec, command, params, false, false);
    }

    @Override
    public <T, R> RFuture<R> evalWriteAsync(String key, Codec codec, RedisCommand<T> evalCommandType, String script, List<Object> keys, Object... params) {
        NodeSource source = getNodeSource(key);
        return evalAsync(source, false, codec, evalCommandType, script, keys, false, params);
    }

    @Override
    public <T, R> RFuture<R> evalWriteNoRetryAsync(String key, Codec codec, RedisCommand<T> evalCommandType, String script, List<Object> keys, Object... params) {
        NodeSource source = getNodeSource(key);
        return evalAsync(source, false, codec, evalCommandType, script, keys, true, params);
    }

    private RFuture<String> loadScript(RedisClient client, String script) {
        MasterSlaveEntry entry = getConnectionManager().getEntry(client);
        if (entry.getClient().equals(client)) {
            return writeAsync(entry, StringCodec.INSTANCE, RedisCommands.SCRIPT_LOAD, script);
        }
        return readAsync(client, StringCodec.INSTANCE, RedisCommands.SCRIPT_LOAD, script);
    }

    protected boolean isEvalCacheActive() {
        return connectionManager.getServiceManager().getCfg().isUseScriptCache();
    }

    protected final List<Object> copy(List<Object> params) {
        List<Object> result = new ArrayList<>(params.size());
        for (Object object : params) {
            if (object instanceof ByteBuf) {
                ByteBuf b = (ByteBuf) object;
                ByteBuf nb = b.copy();
                result.add(nb);
            } else {
                result.add(object);
            }
        }
        return result;
    }

    private static String trunc(String input) {
        final int maxLength = 200;

        if (input == null) {
            return null;
        }

        if (input.length() <= maxLength) {
            return input;
        }

        return input.substring(0, maxLength) + "...";
    }

    protected final Object[] copy(Object[] params) {
        return copy(Arrays.asList(params)).toArray();
    }

    private static final AtomicBoolean EVAL_SHA_RO_SUPPORTED = new AtomicBoolean(true);

    private static final Pattern COMMANDS_PATTERN = Pattern.compile("redis\\.call\\(['\"]{1}([\\w.]+)['\"]{1}");

    private String map(String script) {
        if (getServiceManager().getCommandMapper() instanceof DefaultCommandMapper) {
            return script;
        }

        Matcher matcher = COMMANDS_PATTERN.matcher(script);
        Set<String> mappedCommands = new HashSet<>();
        while (matcher.find()) {

            String command = matcher.group(1);
            if (mappedCommands.contains(command)) {
                continue;
            }
            String mapped = getServiceManager().getCommandMapper().map(command);
            if (!command.equalsIgnoreCase(mapped)) {
                script = script.replace(command, mapped);
                mappedCommands.add(command);
            }
        }
        return script;
    }

    public <T, R> RFuture<R> evalAsync(NodeSource nodeSource, boolean readOnlyMode, Codec codec, RedisCommand<T> evalCommandType,
                                       String script, List<Object> keys, boolean noRetry, Object... params) {

        String mappedScript = map(script);

        if (isEvalCacheActive() && evalCommandType.getName().equals("EVAL")) {
            CompletableFuture<R> mainPromise = new CompletableFuture<>();

            List<Object> keysCopy = copy(keys);
            Object[] paramsCopy = copy(params);

            CompletableFuture<R> promise = new CompletableFuture<>();
            String sha1 = getServiceManager().calcSHA(mappedScript);
            RedisCommand cmd;
            if (readOnlyMode && EVAL_SHA_RO_SUPPORTED.get()) {
                cmd = new RedisCommand(evalCommandType, "EVALSHA_RO", trunc(mappedScript));
            } else {
                cmd = new RedisCommand(evalCommandType, "EVALSHA", trunc(mappedScript));
            }
            List<Object> args = new ArrayList<Object>(2 + keys.size() + params.length);
            args.add(sha1);
            args.add(keys.size());
            args.addAll(keys);
            args.addAll(Arrays.asList(params));

            RedisExecutor<T, R> executor = new RedisExecutor(readOnlyMode, nodeSource, codec, cmd,
                    args.toArray(), promise, false,
                    connectionManager, noRetry,
                    retryAttempts, retryDelay, responseTimeout, trackChanges);
            executor.execute();

            promise.whenComplete((res, e) -> {
                if (e != null) {
                    if (e.getMessage().startsWith("ERR unknown command")) {
                        EVAL_SHA_RO_SUPPORTED.set(false);
                        RFuture<R> future = evalAsync(nodeSource, readOnlyMode, codec, evalCommandType, mappedScript, keysCopy, noRetry, paramsCopy);
                        transfer(future.toCompletableFuture(), mainPromise);
                    } else if (e.getMessage().startsWith("NOSCRIPT")) {
                        RFuture<String> loadFuture = loadScript(executor.getRedisClient(), mappedScript);
                        loadFuture.whenComplete((r, ex) -> {
                            if (ex != null) {
                                free(keysCopy);
                                free(paramsCopy);
                                mainPromise.completeExceptionally(ex);
                                return;
                            }

                            List<Object> newargs = new ArrayList<Object>(2 + keys.size() + params.length);
                            newargs.add(sha1);
                            newargs.add(keys.size());
                            newargs.addAll(keysCopy);
                            newargs.addAll(Arrays.asList(paramsCopy));

                            NodeSource ns = nodeSource;
                            if (ns.getRedisClient() == null) {
                                ns = new NodeSource(nodeSource, executor.getRedisClient());
                            }

                            RFuture<R> future = asyncNoScript(readOnlyMode, ns, codec, cmd, newargs.toArray(), false, noRetry);
                            transfer(future.toCompletableFuture(), mainPromise);
                        });
                    } else {
                        free(keysCopy);
                        free(paramsCopy);
                        mainPromise.completeExceptionally(e);
                    }
                    return;
                }
                free(keysCopy);
                free(paramsCopy);
                mainPromise.complete(res);
            });
            return new CompletableFutureWrapper<>(mainPromise);
        }

        List<Object> args = new ArrayList<Object>(2 + keys.size() + params.length);
        args.add(mappedScript);
        args.add(keys.size());
        args.addAll(keys);
        args.addAll(Arrays.asList(params));
        return async(readOnlyMode, nodeSource, codec, evalCommandType, args.toArray(), false, noRetry);
    }

    protected <V, R> RFuture<R> asyncNoScript(boolean readOnlyMode, NodeSource source, Codec codec,
                                              RedisCommand<V> command, Object[] params, boolean ignoreRedirect, boolean noRetry) {
        return async(readOnlyMode, source, codec, command, params, ignoreRedirect, noRetry);
    }

    @Override
    public <T, R> RFuture<R> writeAsync(String key, Codec codec, RedisCommand<T> command, Object... params) {
        NodeSource source = getNodeSource(key);
        return async(false, source, codec, command, params, false, false);
    }

    public <T, R> RFuture<R> writeAsync(byte[] key, Codec codec, RedisCommand<T> command, Object... params) {
        NodeSource source = getNodeSource(key);
        return async(false, source, codec, command, params, false, false);
    }

    private static final AtomicBoolean SORT_RO_SUPPORTED = new AtomicBoolean(true);

    public <V, R> RFuture<R> async(boolean readOnlyMode, NodeSource source, Codec codec,
                                   RedisCommand<V> command, Object[] params, boolean ignoreRedirect, boolean noRetry) {
        RedisCommand<V> cmnd = command;

        if (readOnlyMode && cmnd.getName().equals("SORT") && !SORT_RO_SUPPORTED.get()) {
            readOnlyMode = false;
        } else if (readOnlyMode && cmnd.getName().equals("SORT") && SORT_RO_SUPPORTED.get()) {
            RedisCommand cmd = new RedisCommand("SORT_RO", cmnd.getReplayMultiDecoder());
            CompletableFuture<R> mainPromise = createPromise();
            RedisExecutor<V, R> executor = new RedisExecutor<>(readOnlyMode, source, codec, cmd, params, mainPromise,
                    ignoreRedirect, connectionManager, noRetry,
                    retryAttempts, retryDelay, responseTimeout, trackChanges);
            executor.execute();
            CompletableFuture<R> result = new CompletableFuture<>();
            mainPromise.whenComplete((r, e) -> {
                if (e != null && e.getMessage().startsWith("ERR unknown command")) {
                    SORT_RO_SUPPORTED.set(false);
                    RFuture<R> future = async(false, source, codec, command, params, ignoreRedirect, noRetry);
                    transfer(future.toCompletableFuture(), result);
                    return;
                }
                transfer(mainPromise, result);
            });
            return new CompletableFutureWrapper<>(result);
        }

        CompletableFuture<R> mainPromise = createPromise();
        RedisExecutor<V, R> executor = new RedisExecutor<>(readOnlyMode, source, codec, cmnd, params, mainPromise,
                ignoreRedirect, connectionManager, noRetry,
                retryAttempts, retryDelay, responseTimeout, trackChanges);
        executor.execute();
        return new CompletableFutureWrapper<>(mainPromise);
    }

    private void free(Object[] params) {
        for (Object obj : params) {
            ReferenceCountUtil.safeRelease(obj);
        }
    }

    private void free(List<Object> params) {
        for (Object obj : params) {
            ReferenceCountUtil.safeRelease(obj);
        }
    }

    public ServiceManager getServiceManager() {
        return connectionManager.getServiceManager();
    }

    @Override
    public <T> CompletionStage<T> handleNoSync(CompletionStage<T> stage, Function<Throwable, CompletionStage<?>> supplier) {
        CompletionStage<T> s = stage.handle((r, ex) -> {
            if (ex != null) {
                if (ex.getCause() instanceof NoSyncedSlavesException) {
                    return supplier.apply(ex.getCause()).handle((r1, e) -> {
                        if (e != null) {
                            if (e.getCause() instanceof NoSyncedSlavesException) {
                                throw new CompletionException(ex.getCause());
                            }
                            if (e.getCause() != null) {
                                e.getCause().addSuppressed(ex.getCause());
                            } else {
                                e.addSuppressed(ex.getCause());
                            }
                        }
                        throw new CompletionException(ex.getCause());
                    });
                } else {
                    if (ex.getCause() != null) {
                        throw new CompletionException(ex.getCause());
                    }
                    throw new CompletionException(ex);
                }
            }
            return CompletableFuture.completedFuture(r);
        }).thenCompose(f -> (CompletionStage<T>) f);
        return s;
    }

    @Override
    public <T> RFuture<T> syncedEvalWithRetry(String key, Codec codec, RedisCommand<T> evalCommandType, String script, List<Object> keys, Object... params) {
        return getServiceManager().execute(() -> syncedEval(key, codec, evalCommandType, script, keys, params));
    }

    @Override
    public <T> RFuture<T> syncedEvalNoRetry(String key, Codec codec, RedisCommand<T> evalCommandType, String script, List<Object> keys, Object... params) {
        return syncedEval(false, key, codec, evalCommandType, script, keys, params);
    }

    @Override
    public <T> RFuture<T> syncedEval(String key, Codec codec, RedisCommand<T> evalCommandType, String script, List<Object> keys, Object... params) {
        return syncedEval(true, key, codec, evalCommandType, script, keys, params);
    }

    private <T> RFuture<T> syncedEval(boolean retry, String key, Codec codec, RedisCommand<T> evalCommandType,
                                      String script, List<Object> keys, Object... params) {
        if (retry) {
            return evalWriteAsync(key, codec, evalCommandType, script, keys, params);
        }
        return evalWriteNoRetryAsync(key, codec, evalCommandType, script, keys, params);
    }

}
