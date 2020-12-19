#include "unity.h"
#include "casper_client.h"

void setUp(void) {
    casper_setup_client();
}

void tearDown(void) {
    casper_shutdown_client();
}

void test_should_be_no_last_error_on_startup(void) {
    unsigned char error[255] = {0};
    int bytes_read = casper_get_last_error(error, 255);
    TEST_ASSERT_EQUAL_INT(0, bytes_read);
}

void test_should_get_last_error_after_bad_request(void) {
    casper_deploy_params_t deploy_params = {0};
    deploy_params.secret_key = "resources/local/secret_keys/node-1.pem";
    deploy_params.ttl = "10s";
    deploy_params.chain_name = "casper-charlie-testnet1";
    deploy_params.gas_price = "11";

    casper_payment_params_t payment_params = {0};
    payment_params.payment_amount = "1000";

    const char *payment_args[2] = {
        "name_01:bool='false'",
        "name_02:i32='42'",
    };
    payment_params.payment_args_simple = (const char *const *)&payment_args;
    payment_params.payment_args_simple_len = 2;

    casper_session_params_t session_params = {0};
    session_params.session_path =
        "target/wasm32-unknown-unknown/release/standard_payment.wasm";

    unsigned char response_buffer[1024] = {0};

    casper_error_t success =
        casper_put_deploy("1", "", false, &deploy_params, &session_params,
                          &payment_params, response_buffer, 1024);

    TEST_ASSERT_NOT_EQUAL_INT(CASPER_SUCCESS, success);

    unsigned char error[255] = {0};
    int bytes_read = casper_get_last_error(error, 255);
    TEST_ASSERT_EQUAL_STRING("failed to get rpc response: builder error: relative URL without a base",
                             error);
}

int main(int argc, char **argv) {
    UNITY_BEGIN();
    RUN_TEST(test_should_be_no_last_error_on_startup);
    RUN_TEST(test_should_get_last_error_after_bad_request);
    return UNITY_END();
}
