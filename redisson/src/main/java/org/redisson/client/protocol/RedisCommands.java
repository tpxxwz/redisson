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
package org.redisson.client.protocol;

import java.util.Arrays;
import java.util.Collections;
import java.util.HashMap;
import java.util.HashSet;
import java.util.List;
import java.util.Map;
import java.util.Set;

import org.redisson.api.*;
import org.redisson.client.codec.Codec;
import org.redisson.client.codec.DoubleCodec;
import org.redisson.client.codec.StringCodec;
import org.redisson.client.handler.State;
import org.redisson.client.protocol.convertor.*;
import org.redisson.client.protocol.decoder.*;
import org.redisson.client.protocol.pubsub.PubSubStatusDecoder;

/**
 * 
 * @author Nikita Koksharov
 *
 */
public interface RedisCommands {

    RedisStrictCommand<Long> GEOADD = new RedisStrictCommand<Long>("GEOADD");
    RedisStrictCommand<Integer> WAIT = new RedisStrictCommand<Integer>("WAIT", new IntegerReplayConvertor());
    RedisCommand<List<Integer>> WAITAOF = new RedisCommand("WAITAOF", new ObjectListReplayDecoder<Integer>(), new IntegerReplayConvertor());
    RedisStrictCommand<Void> ASKING = new RedisStrictCommand<Void>("ASKING", new VoidReplayConvertor());
    RedisStrictCommand<Void> READONLY = new RedisStrictCommand<Void>("READONLY", new VoidReplayConvertor());
    RedisStrictCommand<Double> ZINCRBY = new RedisStrictCommand<Double>("ZINCRBY", new DoubleNullSafeReplayConvertor());

    RedisCommand<String> PING = new RedisCommand<String>("PING");
    RedisStrictCommand<Void> MULTI = new RedisStrictCommand<Void>("MULTI", new VoidReplayConvertor());
    RedisCommand<List<Object>> EXEC = new RedisCommand<List<Object>>("EXEC", new ObjectListReplayDecoder<Object>());
    RedisCommand<Object> LPOP = new RedisCommand<Object>("LPOP");

    RedisStrictCommand<Long> PTTL = new RedisStrictCommand<Long>("PTTL");

    RedisCommand<Object> RPOPLPUSH = new RedisCommand<Object>("RPOPLPUSH");

    RedisCommand<Object> RPOP = new RedisCommand<Object>("RPOP");
    RedisCommand<Integer> LPUSH = new RedisCommand<Integer>("LPUSH", new IntegerReplayConvertor());
    RedisCommand<Integer> LPUSHX = new RedisCommand<Integer>("LPUSHX", new IntegerReplayConvertor());
    RedisCommand<Integer> RPUSH = new RedisCommand<Integer>("RPUSH", new IntegerReplayConvertor());
    RedisCommand<Integer> RPUSHX = new RedisCommand<Integer>("RPUSHX", new IntegerReplayConvertor());
    RedisStrictCommand<String> SCRIPT_LOAD = new RedisStrictCommand<String>("SCRIPT", "LOAD", new ObjectDecoder(new StringDataDecoder()));
    RedisStrictCommand<Boolean> EVAL_BOOLEAN = new RedisStrictCommand<Boolean>("EVAL", new BooleanReplayConvertor());
    RedisStrictCommand<Boolean> EVAL_NULL_BOOLEAN = new RedisStrictCommand<Boolean>("EVAL", new BooleanNullReplayConvertor());
    RedisStrictCommand<Long> EVAL_LONG = new RedisStrictCommand<Long>("EVAL");

    RedisStrictCommand<Long> INCR = new RedisStrictCommand<Long>("INCR");
    RedisStrictCommand<Long> INCRBY = new RedisStrictCommand<Long>("INCRBY");
    RedisStrictCommand<Long> DECR = new RedisStrictCommand<Long>("DECR");


    RedisStrictCommand<Map<String, String>> HELLO = new RedisStrictCommand<>("HELLO", new StringMapReplayDecoder());
    RedisStrictCommand<Void> AUTH = new RedisStrictCommand<Void>("AUTH", new VoidReplayConvertor());
    RedisStrictCommand<Void> SELECT = new RedisStrictCommand<Void>("SELECT", new VoidReplayConvertor());

    RedisStrictCommand<Long> CLIENT_ID = new RedisStrictCommand<>("CLIENT", "ID");

    RedisStrictCommand<Void> CLIENT_TRACKING = new RedisStrictCommand<Void>("CLIENT", "TRACKING", new VoidReplayConvertor());

    RedisStrictCommand<Void> CLIENT_CAPA = new RedisStrictCommand<Void>("CLIENT", "CAPA", new VoidReplayConvertor());
    RedisStrictCommand<Void> CLIENT_SETNAME = new RedisStrictCommand<Void>("CLIENT", "SETNAME", new VoidReplayConvertor());
    RedisCommand<List<Object>> MGET = new RedisCommand<List<Object>>("MGET", new ObjectListReplayDecoder<Object>());
    RedisStrictCommand<Boolean> MSETNX = new RedisStrictCommand<Boolean>("MSETNX", new BooleanReplayConvertor());

    RedisStrictCommand<Boolean> HSETNX = new RedisStrictCommand<Boolean>("HSETNX", new BooleanReplayConvertor());
    RedisCommand<Boolean> HEXISTS = new RedisCommand<Boolean>("HEXISTS", new BooleanReplayConvertor());
    RedisStrictCommand<Long> DEL = new RedisStrictCommand<Long>("DEL");
    RedisStrictCommand<Boolean> DEL_BOOL = new RedisStrictCommand<Boolean>("DEL", new BooleanNullSafeReplayConvertor());
    RedisCommand<Void> APPEND = new RedisCommand<Void>("APPEND", new VoidReplayConvertor());
    RedisCommand<Boolean> SET_BOOLEAN = new RedisCommand<Boolean>("SET", new BooleanNotNullReplayConvertor());
    RedisCommand<Boolean> SETNX = new RedisCommand<Boolean>("SETNX", new BooleanReplayConvertor());

    RedisStrictCommand<Boolean> EXISTS = new RedisStrictCommand<Boolean>("EXISTS", new BooleanAmountReplayConvertor());

    RedisStrictCommand<Long> PUBLISH = new RedisStrictCommand<Long>("PUBLISH");

    RedisStrictCommand<Long> SPUBLISH = new RedisStrictCommand<Long>("SPUBLISH");
    RedisCommand<Long> PUBSUB_NUMPAT = new RedisCommand<>("PUBSUB", "NUMPAT", new ListObjectDecoder<>(1));
    RedisCommand<List<String>> PUBSUB_CHANNELS = new RedisStrictCommand<>("PUBSUB", "CHANNELS", new StringListReplayDecoder());
    RedisCommand<Long> PUBSUB_SHARDNUMSUB = new RedisCommand<>("PUBSUB", "SHARDNUMSUB", new ListObjectDecoder<>(1));

    RedisCommand<Object> SSUBSCRIBE = new RedisCommand<>("SSUBSCRIBE", new PubSubStatusDecoder());
    RedisCommand<Object> SUBSCRIBE = new RedisCommand<>("SUBSCRIBE", new PubSubStatusDecoder());
    RedisCommand<Object> UNSUBSCRIBE = new RedisCommand<>("UNSUBSCRIBE", new PubSubStatusDecoder());
    RedisCommand<Object> SUNSUBSCRIBE = new RedisCommand<>("SUNSUBSCRIBE", new PubSubStatusDecoder());
    RedisCommand<Object> PSUBSCRIBE = new RedisCommand<>("PSUBSCRIBE", new PubSubStatusDecoder());
    RedisCommand<Object> PUNSUBSCRIBE = new RedisCommand<>("PUNSUBSCRIBE", new PubSubStatusDecoder());

    Set<String> PUBSUB_COMMANDS = Collections.unmodifiableSet(new HashSet<>(
            Arrays.asList(PSUBSCRIBE.getName(), SUBSCRIBE.getName(), PUNSUBSCRIBE.getName(),
                    UNSUBSCRIBE.getName(), SSUBSCRIBE.getName(), SUNSUBSCRIBE.getName())));
    RedisCommand<List<Map<String, String>>> SENTINEL_SLAVES = new RedisCommand<List<Map<String, String>>>("SENTINEL", "SLAVES",
            new ListMultiDecoder2(new ListResultReplayDecoder(), new ObjectMapReplayDecoder()));
    RedisCommand<List<Map<String, String>>> SENTINEL_SENTINELS = new RedisCommand<List<Map<String, String>>>("SENTINEL", "SENTINELS",
            new ListMultiDecoder2(new ListResultReplayDecoder(), new ObjectMapReplayDecoder()));

    RedisStrictCommand<Map<String, String>> INFO_REPLICATION = new RedisStrictCommand<Map<String, String>>("INFO", "REPLICATION", new StringMapDataDecoder());

    Set<RedisCommand> NO_RETRY_COMMANDS = new HashSet<>(Arrays.asList(SET_BOOLEAN));

    Set<String> NO_RETRY = new HashSet<>(
            Arrays.asList(RPOPLPUSH.getName(), LPOP.getName(), RPOP.getName(), LPUSH.getName(), RPUSH.getName(),
                    LPUSHX.getName(), RPUSHX.getName(), GEOADD.getName(), APPEND.getName(),
                    DECR.getName(), "DECRBY", INCR.getName(), INCRBY.getName(), ZINCRBY.getName(),
                    "HINCRBYFLOAT", "HINCRBY", "INCRBYFLOAT", SETNX.getName(), MSETNX.getName(), HSETNX.getName()));


}
