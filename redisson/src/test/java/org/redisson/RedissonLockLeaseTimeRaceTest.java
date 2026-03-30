package org.redisson;

import org.junit.jupiter.api.RepeatedTest;
import org.junit.jupiter.api.Test;
import org.redisson.api.RLock;

import java.lang.reflect.Field;
import java.util.ArrayList;
import java.util.List;
import java.util.concurrent.*;
import java.util.concurrent.atomic.AtomicInteger;

import static org.assertj.core.api.Assertions.assertThat;

/**
 * 验证 RedissonLock.internalLockLeaseTime 的线程安全问题。
 *
 * <p>Java 使用普通 {@code long} 字段（无 volatile / synchronized），多个线程共享同一个
 * {@code RLock} 对象并以不同的 leaseTime 调用 tryLock 时，对该字段的写入存在数据竞争：
 * <ol>
 *   <li>tryAcquireAsync thenApply 回调（Netty I/O 线程）写入 {@code leaseTime}</li>
 *   <li>cancelExpirationRenewal 回调写入 {@code watchdogTimeout}</li>
 * </ol>
 * 两者可能交错，导致 internalLockLeaseTime 落地的值不确定。
 *
 * <p>本测试通过反射读取该私有字段，演示：
 * <ul>
 *   <li>单线程场景：值与预期一致</li>
 *   <li>多线程共享同一 RLock 对象场景：经过足够多次迭代后，字段值存在不确定性</li>
 * </ul>
 */
public class RedissonLockLeaseTimeRaceTest extends RedisDockerTest {

    private static final String LOCK_KEY = "race-test-lock";

    // -----------------------------------------------------------------------
    // 工具方法：通过反射读取 internalLockLeaseTime
    // -----------------------------------------------------------------------

    private static long readLeaseTime(RLock lock) throws Exception {
        Field f = RedissonLock.class.getDeclaredField("internalLockLeaseTime");
        f.setAccessible(true);
        return (long) f.get(lock);
    }

    // -----------------------------------------------------------------------
    // 1. 基线测试：单线程，leaseTime 应与传入值一致
    // -----------------------------------------------------------------------

    @Test
    void singleThread_leaseTimeMatchesSpecified() throws Exception {
        RLock lock = redisson.getLock(LOCK_KEY);
        long leaseTime = 5_000L;

        boolean acquired = lock.tryLock(0, leaseTime, TimeUnit.MILLISECONDS);
        assertThat(acquired).isTrue();
        try {
            long actual = readLeaseTime(lock);
            assertThat(actual)
                    .as("单线程下 internalLockLeaseTime 应等于指定的 leaseTime")
                    .isEqualTo(leaseTime);
        } finally {
            lock.unlock();
        }
    }

    // -----------------------------------------------------------------------
    // 2. 多线程共享同一 RLock 对象：leaseTime 写入存在竞争
    //
    // 场景：N 个线程轮流持有同一把锁，每个线程指定不同的 leaseTime（各不相同），
    // 记录每次 tryLock 成功后立即读取的 internalLockLeaseTime 值。
    //
    // 预期（如果没有竞争）：读到的值 == 本线程传入的 leaseTime。
    // 实际（有竞争时）    ：读到的值可能是其他线程写入的 leaseTime 或 watchdogTimeout，
    //                       即 assertThat 断言失败的次数 > 0。
    // -----------------------------------------------------------------------

    @RepeatedTest(3)
    void multiThread_sharedLock_leaseTimeIsNonDeterministic() throws Exception {
        RLock lock = redisson.getLock(LOCK_KEY);

        int threads = 4;
        int iterations = 20;                 // 每线程轮转次数
        long baseLeaseTime = 3_000L;         // 各线程 leaseTime = base + threadIndex * 1000
        long watchdogTimeout = redisson.getConfig().getLockWatchdogTimeout();

        AtomicInteger mismatchCount = new AtomicInteger(0);
        AtomicInteger totalAcquired = new AtomicInteger(0);
        List<Future<?>> futures = new ArrayList<>();
        ExecutorService pool = Executors.newFixedThreadPool(threads);

        for (int i = 0; i < threads; i++) {
            final long leaseTime = baseLeaseTime + i * 1_000L;
            futures.add(pool.submit(() -> {
                for (int iter = 0; iter < iterations; iter++) {
                    try {
                        // waitTime 足够长以确保最终能拿到锁
                        boolean acquired = lock.tryLock(30_000, leaseTime, TimeUnit.MILLISECONDS);
                        if (!acquired) continue;
                        totalAcquired.incrementAndGet();
                        try {
                            long actual = readLeaseTime(lock);
                            // 理论上应等于本线程传入的 leaseTime；
                            // 若因竞争读到不一致的值则记录
                            if (actual != leaseTime) {
                                mismatchCount.incrementAndGet();
                                System.out.printf(
                                        "[MISMATCH] thread-leaseTime=%d  actual internalLockLeaseTime=%d  (watchdog=%d)%n",
                                        leaseTime, actual, watchdogTimeout);
                            }
                        } finally {
                            lock.unlock();
                        }
                    } catch (Exception e) {
                        Thread.currentThread().interrupt();
                    }
                }
            }));
        }

        pool.shutdown();
        pool.awaitTermination(120, TimeUnit.SECONDS);

        System.out.printf(
                "total acquired=%d  mismatches=%d (%.1f%%)%n",
                totalAcquired.get(),
                mismatchCount.get(),
                totalAcquired.get() == 0 ? 0.0
                        : mismatchCount.get() * 100.0 / totalAcquired.get());

        // 此断言用来记录观测结果，不强制通过/失败：
        // 若 mismatch > 0 说明竞争确实发生了；
        // 若 mismatch == 0 说明本次运行没有触发（概率性问题，重复运行可能出现）。
        System.out.printf(
                "结论：internalLockLeaseTime 在本次运行中%s发生竞态写入%n",
                mismatchCount.get() > 0 ? "【已】" : "【未】（概率性，可重复运行）");
    }

    // -----------------------------------------------------------------------
    // 3. 明确复现：Thread-1 持有锁期间 Thread-2 排队等待，
    //    Thread-1 unlock 的 cancelExpirationRenewal 与 Thread-2 的 thenApply 回调竞争
    // -----------------------------------------------------------------------

    @RepeatedTest(5)
    void cancelExpirationRenewal_vs_tryAcquireCallback_race() throws Exception {
        RLock lock = redisson.getLock(LOCK_KEY);

        long leaseTime1 = 4_000L;
        long leaseTime2 = 20_000L;
        long watchdogTimeout = redisson.getConfig().getLockWatchdogTimeout();

        CountDownLatch t1Acquired = new CountDownLatch(1);
        CountDownLatch t1CanUnlock = new CountDownLatch(1);
        CountDownLatch done = new CountDownLatch(2);

        // 记录 Thread-2 刚拿到锁后读取到的 internalLockLeaseTime
        long[] leaseAfterT2Acquired = {-1L};

        Thread t1 = new Thread(() -> {
            try {
                lock.lock(leaseTime1, TimeUnit.MILLISECONDS);
                t1Acquired.countDown();
                t1CanUnlock.await();
                // unlock → cancelExpirationRenewal → internalLockLeaseTime = watchdogTimeout
                lock.unlock();
            } catch (InterruptedException e) {
                Thread.currentThread().interrupt();
            } finally {
                done.countDown();
            }
        }, "t1");

        Thread t2 = new Thread(() -> {
            try {
                t1Acquired.await();
                t1CanUnlock.countDown();
                // Thread-2 排队，等 t1 释放后触发 tryAcquire thenApply → internalLockLeaseTime = leaseTime2
                boolean acquired = lock.tryLock(30_000, leaseTime2, TimeUnit.MILLISECONDS);
                if (acquired) {
                    leaseAfterT2Acquired[0] = readLeaseTime(lock);
                    lock.unlock();
                }
            } catch (Exception e) {
                Thread.currentThread().interrupt();
            } finally {
                done.countDown();
            }
        }, "t2");

        t1.start();
        t2.start();
        done.await(30, TimeUnit.SECONDS);

        long actual = leaseAfterT2Acquired[0];
        System.out.printf(
                "leaseTime1=%d  leaseTime2=%d  watchdog=%d  actual-after-t2=%d  match=%s%n",
                leaseTime1, leaseTime2, watchdogTimeout, actual,
                actual == leaseTime2 ? "leaseTime2(预期)" :
                actual == watchdogTimeout ? "watchdog(竞争：cancelExpirationRenewal 后写)" :
                "other=" + actual);

        // 不做硬断言：观察 actual 是否等于 leaseTime2。
        // 若打印出 "watchdog" 说明 cancelExpirationRenewal 的写入覆盖了 thenApply 的写入，竞争已复现。
        assertThat(actual).isNotEqualTo(-1L);
    }
}
