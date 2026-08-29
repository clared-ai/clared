"""Compare an unsafe multi-step workflow with Clared's in-memory reference simulator."""

import asyncio
import os

from clared import ClaredHarness


SECRET = "0123456789abcdef0123456789abcdef"


def run_unsafe_baseline() -> None:
    order = {"id": "ord_1042", "status": "pending"}
    payment_authorized = False
    notification_sent = False
    print("\n1. Unsafe baseline")
    try:
        order["status"] = "payment_authorized"
        payment_authorized = True
        raise ConnectionError("injected network failure before notification")
    except ConnectionError as error:
        print("   database update: committed immediately")
        print("   payment: authorized (hold left open)")
        print(f"   injected failure: {error}")
    print(f"   order status: {order['status']}")
    print(f"   notification sent: {notification_sent}")
    print("   rollback: none")
    print("   result: inconsistent state")


async def run_clared_abort(harness: ClaredHarness) -> None:
    print("\n2. Clared path with the same injected failure")
    session = None
    try:
        async with harness.session(
            tenant_id="acme_corp",
            principal="alice",
            agent_role="checkout_agent",
            task_intent="authorize_order_1042",
            target_resources=["order:ord_1042", "customer:cus_9918"],
            allowed_tools=[
                "postgres.orders.update",
                "stripe.payment_intents.create",
                "twilio.messages.create",
            ],
            budgets={
                "money.minor.USD.hold": 50_000,
                "money.minor.USD.capture": 50_000,
                "database.mutations.count": 1,
                "external_notifications.count": 1,
            },
        ) as active_session:
            session = active_session
            database = await session.call_tool(
                "postgres.orders.update",
                {"order_id": "ord_1042", "status": "payment_authorized"},
                idempotency_key="fault-demo-db",
            )
            payment = await session.call_tool(
                "stripe.payment_intents.create",
                {"customer_id": "cus_9918", "amount_minor": 45_000},
                idempotency_key="fault-demo-payment",
            )
            print(f"   database: {database['status']}")
            print(f"   payment: {payment['status']}")
            raise ConnectionError("injected network failure before notification")
    except ConnectionError as error:
        print(f"   injected failure: {error}")

    if session is None or session.abort_receipt is None:
        raise RuntimeError("Clared did not return an abort receipt")
    receipt = session.abort_receipt
    print(f"   final state: {receipt['status']}")
    print(f"   reverted simulated actions: {receipt['reverted_actions_count']}")
    print(f"   escaped side effects: {receipt['escaped_side_effects']}")


async def run_clared_success(harness: ClaredHarness) -> None:
    print("\n3. Successful simulated settlement")
    session = None
    async with harness.session(
        tenant_id="acme_corp",
        principal="alice",
        agent_role="checkout_agent",
        task_intent="authorize_order_1042",
        target_resources=["order:ord_1042", "customer:cus_9918"],
        allowed_tools=[
            "postgres.orders.update",
            "stripe.payment_intents.create",
            "twilio.messages.create",
        ],
        budgets={
            "money.minor.USD.hold": 50_000,
            "money.minor.USD.capture": 50_000,
            "database.mutations.count": 1,
            "external_notifications.count": 1,
        },
    ) as active_session:
        session = active_session
        await session.call_tool(
            "postgres.orders.update",
            {"order_id": "ord_1042", "status": "payment_authorized"},
            idempotency_key="success-demo-db",
        )
        await session.call_tool(
            "stripe.payment_intents.create",
            {"customer_id": "cus_9918", "amount_minor": 45_000},
            idempotency_key="success-demo-payment",
        )
        await session.call_tool(
            "twilio.messages.create",
            {"to": "+15550192834", "body": "Your order was authorized."},
            idempotency_key="success-demo-sms",
        )

    if session is None or session.settlement_receipt is None:
        raise RuntimeError("Clared did not return a settlement receipt")
    receipt = session.settlement_receipt
    print(f"   final state: {receipt['status']}")
    print(f"   backend: {receipt['execution_backend']}")
    print(f"   evidence: {receipt['evidence_hash']}")
    print(f"   signature: {receipt['evidence_signature'][:32]}...")


async def main() -> None:
    if os.environ.get("CLARED_DELEGATION_SECRET") != SECRET:
        raise RuntimeError(
            "Set CLARED_DELEGATION_SECRET=0123456789abcdef0123456789abcdef "
            "for both the server and this demo"
        )
    print("Clared fault-injection reference demo")
    print("Backend: in-memory simulator; no external systems are contacted")
    run_unsafe_baseline()
    harness = ClaredHarness()
    await run_clared_abort(harness)
    await run_clared_success(harness)


if __name__ == "__main__":
    asyncio.run(main())
