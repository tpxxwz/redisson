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
package org.redisson;

import org.redisson.api.*;
import org.redisson.client.codec.Codec;
import org.redisson.client.codec.StringCodec;
import org.redisson.client.protocol.RedisCommands;
import org.redisson.command.CommandAsyncExecutor;
import org.redisson.connection.ServiceManager;
import org.redisson.pubsub.PublishSubscribeService;

import java.util.*;
import java.util.stream.Collectors;

/**
 * Base Redisson object
 *
 * @author Nikita Koksharov
 *
 */
public abstract class RedissonObject implements RObject {

    protected CommandAsyncExecutor commandExecutor;
    protected String name;
    protected final Codec codec;

    public RedissonObject(Codec codec, CommandAsyncExecutor commandExecutor, String name) {
        this.codec = commandExecutor.getServiceManager().getCodec(codec);
        this.commandExecutor = commandExecutor;
        if (name == null) {
            throw new NullPointerException("name can't be null");
        }

        setName(name);
    }

    public RedissonObject(CommandAsyncExecutor commandExecutor, String name) {
        this(commandExecutor.getServiceManager().getCfg().getCodec(), commandExecutor, name);
    }

    public static String prefixName(String prefix, String name) {
        if (name.contains("{")) {
            return prefix + ":" + name;
        }
        return prefix + ":{" + name + "}";
    }

    public ServiceManager getServiceManager() {
        return commandExecutor.getServiceManager();
    }

    public static String suffixName(String name, String suffix) {
        if (name.contains("{")) {
            return name + ":" + suffix;
        }
        return "{" + name + "}:" + suffix;
    }

    protected final <V> V get(RFuture<V> future) {
        return commandExecutor.get(future);
    }

    protected final String mapName(String name) {
        return commandExecutor.getServiceManager().getNameMapper().map(name);
    }

    @Override
    public String getName() {
        return commandExecutor.getServiceManager().getNameMapper().unmap(name);
    }

    public final String getRawName() {
        return name;
    }

    protected void setName(String name) {
        this.name = mapName(name);
    }

    @Override
    public boolean isExists() {
        return get(isExistsAsync());
    }

    @Override
    public RFuture<Boolean> isExistsAsync() {
        return commandExecutor.readAsync(getRawName(), StringCodec.INSTANCE, RedisCommands.EXISTS, getRawName());
    }

    @Override
    public RFuture<Boolean> deleteAsync() {
        return commandExecutor.writeAsync(getRawName(), StringCodec.INSTANCE, RedisCommands.DEL_BOOL, getRawName());
    }

    protected final List<String> map(String[] keys) {
        return Arrays.stream(keys)
                .map(k -> mapName(k))
                .collect(Collectors.toList());
    }

    protected final PublishSubscribeService getSubscribeService() {
        return commandExecutor.getConnectionManager().getSubscribeService();
    }

}
