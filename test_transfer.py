import unittest

import transfer


class FakeType:
    def __init__(self, text):
        self.text = text

    def __str__(self):
        return self.text


class FakeFunction:
    def __init__(self, start, name, type_text="int ()", comment=""):
        self.start = start
        self.name = name
        self.type = FakeType(type_text)
        self.comment = comment
        self.vars = []

    def set_user_type(self, value):
        self.type = value


class FakeView:
    def __init__(self, functions):
        self.functions = {f.start: f for f in functions}
        self.began = 0
        self.committed = 0
        self.updated = 0

    def get_function_at(self, address):
        return self.functions.get(address)

    def begin_undo_actions(self):
        self.began += 1
        return "undo-id"

    def commit_undo_actions(self, undo):
        self.assert_undo = undo
        self.committed += 1

    def update_analysis(self):
        self.updated += 1


class NoUndoView(FakeView):
    def begin_undo_actions(self):
        raise RuntimeError("undo unavailable")


class TransferTests(unittest.TestCase):
    def test_plan_and_apply_are_one_undo_action(self):
        source = FakeFunction(0x1000, "parse_record", "int (char*)", "reviewed")
        target = FakeFunction(0x2000, "sub_2000", "void ()")
        view_a, view_b = FakeView([source]), FakeView([target])
        matches = [{
            "function_a": {"address": source.start},
            "function_b": {"address": target.start},
        }]
        options = {key: True for key in transfer.ALL_ATTRS}
        plans = transfer.plan_transfer(view_a, view_b, matches, options)
        summary = transfer.apply_transfer(view_b, plans)

        self.assertEqual(target.name, "parse_record")
        self.assertEqual(str(target.type), "int (char*)")
        self.assertEqual(target.comment, "reviewed")
        self.assertEqual(summary, {"functions": 1, "attributes": 3, "errors": []})
        self.assertEqual((view_b.began, view_b.committed, view_b.updated), (1, 1, 1))

    def test_generated_source_name_is_not_transferred(self):
        source = FakeFunction(0x1000, "sub_1000")
        target = FakeFunction(0x2000, "meaningful")
        plans = transfer.plan_transfer(
            FakeView([source]), FakeView([target]),
            [{"function_a": {"address": 0x1000}, "function_b": {"address": 0x2000}}],
            {transfer.ATTR_NAME: True},
        )
        self.assertEqual(plans, [])

    def test_apply_aborts_when_undo_cannot_start(self):
        target = FakeFunction(0x2000, "before")
        view = NoUndoView([target])
        summary = transfer.apply_transfer(view, [{
            "addr_b": 0x2000,
            "changes": [{"attr": transfer.ATTR_NAME, "new": "after"}],
        }])
        self.assertEqual(target.name, "before")
        self.assertEqual(summary["functions"], 0)
        self.assertIn("undo unavailable", summary["errors"][0])


if __name__ == "__main__":
    unittest.main()
