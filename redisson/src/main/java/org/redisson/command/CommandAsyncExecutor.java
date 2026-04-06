/**
 * Copyright (c) 2013-2026 Nikita Koksharov
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *    http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */
package org.redisson.command;

import io.netty.buffer.ByteBuf;
import org.redisson.api.RFuture;
import org.redisson.api.options.ObjectParams;
import org.redisson.client.RedisClient;
import org.redisson.client.RedisException;
import org.redisson.client.codec.Codec;
import org.redisson.client.protocol.RedisCommand;
import org.redisson.connection.ConnectionManager;
import org.redisson.connection.MasterSlaveEntry;
import org.redisson.connection.ServiceManager;

import java.util.List;
import java.util.concurrent.CompletableFuture;
import java.util.concurrent.CompletionStage;
import java.util.concurrent.ExecutionException;
import java.util.function.Function;

/**
 *
 * @author Nikita Koksharov
 *
 */
public interface CommandAsyncExecutor {

    CommandAsyncExecutor copy(ObjectParams objectParams);

    CommandAsyncExecutor copy(boolean trackChanges);

    ConnectionManager getConnectionManager();

    ServiceManager getServiceManager();

    RedisException convertException(ExecutionException e);

    <V> void transfer(CompletionStage<V> future1, CompletableFuture<V> future2);

    <V> V getNow(CompletableFuture<V> future);

    <V> V get(RFuture<V> future);

    <V> V get(CompletableFuture<V> future);

    <V> V getInterrupted(CompletableFuture<V> future) throws InterruptedException;

    <T, R> RFuture<R> writeAsync(MasterSlaveEntry entry, Codec codec, RedisCommand<T> command, Object... params);

    <T, R> RFuture<R> readAsync(RedisClient client, String name, Codec codec, RedisCommand<T> command, Object... params);

    <T, R> RFuture<R> readAsync(RedisClient client, Codec codec, RedisCommand<T> command, Object... params);

    <T, R> RFuture<R> evalWriteAsync(String key, Codec codec, RedisCommand<T> evalCommandType, String script, List<Object> keys, Object... params);

    <T, R> RFuture<R> evalWriteNoRetryAsync(String key, Codec codec, RedisCommand<T> evalCommandType, String script, List<Object> keys, Object... params);

    <T, R> RFuture<R> readAsync(String key, Codec codec, RedisCommand<T> command, Object... params);

    <T, R> RFuture<R> writeAsync(String key, Codec codec, RedisCommand<T> command, Object... params);

    <T> RFuture<T> syncedEvalWithRetry(String key, Codec codec, RedisCommand<T> evalCommandType, String script, List<Object> keys, Object... params);

    <T> RFuture<T> syncedEvalNoRetry(String key, Codec codec, RedisCommand<T> evalCommandType, String script, List<Object> keys, Object... params);

    <T> RFuture<T> syncedEval(String key, Codec codec, RedisCommand<T> evalCommandType, String script, List<Object> keys, Object... params);

    <T> CompletionStage<T> handleNoSync(CompletionStage<T> stage, Function<Throwable, CompletionStage<?>> supplier);

    static CommandAsyncExecutor create(ConnectionManager connectionManager) {
        return new CommandAsyncService(connectionManager);
    }

}