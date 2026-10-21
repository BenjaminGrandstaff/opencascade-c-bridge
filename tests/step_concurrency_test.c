/* Concurrent STEP exchange from independent sessions. OCCT 7.9's STEP
 * translators share process-wide state; without the bridge's STEP lock,
 * threads making their first STEP calls together crashed or read the healed
 * bowtie fixture as invalid, most often while OCCT initializes its STEP
 * support. So each round is a fresh child process whose threads all start
 * at once: half load the fixture and check it is valid, half write a box. */
#include "occt_bridge.h"

#include <errno.h>
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

static void failed(int index, const char* reason) {
    (void)fprintf(stderr, "thread %d: %s\n", index, reason);
    atomic_fetch_add(&failures, 1);
}

static void load_fixture(occt_bridge_session_t* session, int index) {
    occt_bridge_shape_id_t shape = OCCT_BRIDGE_INVALID_SHAPE_ID;
    int valid = 0;
    if (occt_bridge_step_load(session, fixture, &shape) != OCCT_BRIDGE_OK ||
        occt_bridge_shape_is_valid(session, shape, &valid) != OCCT_BRIDGE_OK || !valid) {
        failed(index, "STEP load failed or was invalid");
    }
}

static void write_fixture(occt_bridge_session_t* session, int index) {
    char path[1024];
    const int length = snprintf(path, sizeof path, "%s/step-concurrency-%d-%d.step", directory, (int)getpid(), index);
    if (length < 0 || (size_t)length >= sizeof path) {
        failed(index, "output path does not fit");
        return;
    }
    const occt_bridge_vec3_t origin = {0.0, 0.0, 0.0};
    const occt_bridge_vec3_t size = {1.0, 2.0, 3.0};
    occt_bridge_shape_id_t shape = OCCT_BRIDGE_INVALID_SHAPE_ID;
    if (occt_bridge_create_box(session, origin, size, &shape) != OCCT_BRIDGE_OK ||
        occt_bridge_step_save(session, shape, path) != OCCT_BRIDGE_OK) {
        failed(index, "STEP write failed");
    }
    if (remove(path) != 0 && errno != ENOENT) {
        failed(index, "could not remove STEP output");
    }
}

static void* exchange(void* argument) {
    const int index = *(const int*)argument;
    occt_bridge_session_t* session = NULL;
    if (occt_bridge_session_create(occt_bridge_abi_version(), &session) != OCCT_BRIDGE_OK) {
        failed(index, "session creation failed");
        atomic_fetch_add(&ready, 1);
        return NULL;
    }
    atomic_fetch_add(&ready, 1);
    while (atomic_load(&ready) < THREADS) {
    }
    if (index % 2 == 0) {
        load_fixture(session, index);
    } else {
        write_fixture(session, index);
    }
    occt_bridge_session_destroy(session);
    return NULL;
}

/* Runs only in a child. Exit immediately on pthread failure, before a live
 * worker's argument can leave scope or its startup barrier lose a thread. */
static int burst(void) {
    pthread_t threads[THREADS];
    int indices[THREADS];
    for (int i = 0; i < THREADS; ++i) {
        indices[i] = i;
        if (pthread_create(&threads[i], NULL, exchange, &indices[i]) != 0) {
            _exit(2);
        }
    }
    for (int i = 0; i < THREADS; ++i) {
        if (pthread_join(threads[i], NULL) != 0) {
            _exit(2);
        }
    }
    return atomic_load(&failures) != 0;
}

static int child_succeeded(pid_t child) {
    int status = 0;
    pid_t waited;
    do {
        waited = waitpid(child, &status, 0);
    } while (waited < 0 && errno == EINTR);
    return waited == child && WIFEXITED(status) && WEXITSTATUS(status) == 0;
}

int main(int argc, char** argv) {
    if (argc != 3) {
        (void)fprintf(stderr, "usage: %s BOWTIE.step OUTPUT_DIRECTORY\n", argv[0]);
        return 2;
    }
    fixture = argv[1];
    directory = argv[2];
    int failed_rounds = 0;
    for (int round = 0; round < ROUNDS; ++round) {
        if (fflush(NULL) != 0) {
            return 2;
        }
        const pid_t child = fork();
        if (child < 0) {
            return 2;
        }
        if (child == 0) {
            _exit(burst());
        }
        if (!child_succeeded(child)) {
            (void)fprintf(stderr, "round %d failed\n", round);
            ++failed_rounds;
        }
    }
    if (failed_rounds != 0) {
        (void)fprintf(stderr, "%d of %d rounds of concurrent STEP calls failed\n", failed_rounds, ROUNDS);
        return 1;
    }
    return 0;
}
