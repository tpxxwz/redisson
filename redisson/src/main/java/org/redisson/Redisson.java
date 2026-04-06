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
package org.redisson;

import org.redisson.api.*;
import org.redisson.api.options.CommonOptions;
import org.redisson.api.options.CommonParams;
import org.redisson.client.codec.Codec;
import org.redisson.command.CommandAsyncExecutor;
import org.redisson.config.Config;
import org.redisson.connection.ConnectionManager;
import org.redisson.connection.ServiceManager;
import org.redisson.renewal.LockRenewalScheduler;
import java.util.concurrent.ConcurrentHashMap;
import java.util.concurrent.ConcurrentMap;
import java.util.concurrent.TimeUnit;

/**
 * Main infrastructure class allows to get access
 * to all Redisson objects on top of Redis server.
 *
 * @author Nikita Koksharov
 *
 */
public final class Redisson implements RedissonClient {

    private final ConnectionManager connectionManager;
    private final CommandAsyncExecutor commandExecutor;

    private final ConcurrentMap<Class<?>, Class<?>> liveObjectClassCache = new ConcurrentHashMap<>();
    private final Config config;

    Redisson(Config config) {
        Version.logVersion();

        this.config = config;
        Config configCopy = new Config(config);

        connectionManager = ConnectionManager.create(configCopy);
        commandExecutor = connectionManager.createCommandExecutor();

        connectionManager.getServiceManager().register(new LockRenewalScheduler(commandExecutor));
    }

    public ServiceManager getServiceManager() {
        return connectionManager.getServiceManager();
    }

    /**
     * Create sync/async Redisson instance with default config
     *
     * @return Redisson instance
     */
    public static RedissonClient create() {
        Config config = new Config();
        config.useSingleServer()
        .setAddress("redis://127.0.0.1:6379");
        return create(config);
    }

    /**
     * Create sync/async Redisson instance with provided config
     *
     * @param config for Redisson
     * @return Redisson instance
     */
    public static RedissonClient create(Config config) {
        return new Redisson(config);
    }

    @Override
    public RLock getLock(String name) {
        return new RedissonLock(commandExecutor, name);
    }

    @Override
    public RLock getLock(CommonOptions options) {
        CommonParams params = (CommonParams) options;
        return new RedissonLock(commandExecutor.copy(params), params.getName());
    }

    @Override
    public void shutdown() {
        connectionManager.shutdown();
    }


    @Override
    public void shutdown(long quietPeriod, long timeout, TimeUnit unit) {
        connectionManager.shutdown(quietPeriod, timeout, unit);
    }

    @Override
    public Config getConfig() {
        return config;
    }

    @Override
    public boolean isShutdown() {
        return connectionManager.getServiceManager().isShutdown();
    }

    @Override
    public boolean isShuttingDown() {
        return connectionManager.getServiceManager().isShuttingDown();
    }

    @Override
    public String getId() {
        return connectionManager.getServiceManager().getId();
    }

}
