/* Concurrent STEP exchange from independent sessions. OCCT 7.9's STEP
 * translators share process-wide state; without the bridge's STEP lock,
 * threads making their first STEP calls together crashed or read the healed
 * bowtie fixture as invalid, most often while OCCT initializes its STEP
 * support. So each round is a fresh child process whose threads all start
 * at once: half load the fixture and check it is valid, half write a box. */
#define _POSIX_C_SOURCE 200809L
#include "occt_bridge.h"

#include <pthread.h>
#include <stdatomic.h>
#include <stdio.h>
#include <sys/wait.h>
#include <unistd.h>

enum { THREADS = 16, ROUNDS = 100 };

static const char* fixture;
static const char* directory;
static atomic_int ready;
static atomic_int failures;

static void* exchange(void* argument) {
    const long index = (long)argument;
    occt_bridge_session_t* session = NULL;
    if (occt_bridge_session_create(occt_bridge_abi_version(), &session) != OCCT_BRIDGE_OK) {
        atomic_fetch_add(&failures, 1);
        atomic_fetch_add(&ready, 1);
        return NULL;
    }
    atomic_fetch_add(&ready, 1);
    while (atomic_load(&ready) < THREADS) {
    }
    occt_bridge_shape_id_t shape = OCCT_BRIDGE_INVALID_SHAPE_ID;
    if (index % 2 == 0) {
        int valid = 0;
        if (occt_bridge_step_load(session, fixture, &shape) != OCCT_BRIDGE_OK ||
            occt_bridge_shape_is_valid(session, shape, &valid) != OCCT_BRIDGE_OK || !valid) {
            fprintf(stderr, "thread %ld: STEP load failed or was invalid\n", index);
            atomic_fetch_add(&failures, 1);
        }
    } else {
        char path[1024];
        snprintf(path, sizeof path, "%s/step-concurrency-%d-%ld.step", directory, (int)getpid(), index);
        const occt_bridge_vec3_t origin = {0.0, 0.0, 0.0};
        const occt_bridge_vec3_t size = {1.0, 2.0, 3.0};
        if (occt_bridge_create_box(session, origin, size, &shape) != OCCT_BRIDGE_OK ||
            occt_bridge_step_save(session, shape, path) != OCCT_BRIDGE_OK) {
            fprintf(stderr, "thread %ld: STEP write failed\n", index);
            atomic_fetch_add(&failures, 1);
        }
        remove(path);
    }
    occt_bridge_session_destroy(session);
    return NULL;
}

/* One burst of concurrent first STEP calls; the exit status counts failures. */
static int burst(void) {
    pthread_t threads[THREADS];
    for (long i = 0; i < THREADS; ++i) {
        if (pthread_create(&threads[i], NULL, exchange, (void*)i) != 0) {
            return 1;
        }
    }
    for (int i = 0; i < THREADS; ++i) {
        pthread_join(threads[i], NULL);
    }
    return atomic_load(&failures) != 0;
}

int main(int argc, char** argv) {
    if (argc != 3) {
        fprintf(stderr, "usage: %s BOWTIE.step OUTPUT_DIRECTORY\n", argv[0]);
        return 2;
    }
    fixture = argv[1];
    directory = argv[2];
    int failed = 0;
    for (int round = 0; round < ROUNDS; ++round) {
        fflush(NULL);
        const pid_t child = fork();
        if (child < 0) {
            return 2;
        }
        if (child == 0) {
            _exit(burst());
        }
        int status = 0;
        if (waitpid(child, &status, 0) != child || !WIFEXITED(status) || WEXITSTATUS(status) != 0) {
            fprintf(stderr, "round %d failed (status %d)\n", round, status);
            ++failed;
        }
    }
    if (failed != 0) {
        fprintf(stderr, "%d of %d rounds of concurrent STEP calls failed\n", failed, ROUNDS);
        return 1;
    }
    return 0;
}
