CREATE TABLE Meta(
    Key TEXT NOT NULL UNIQUE,
    Value INTEGER NOT NULL
);

INSERT INTO Meta(Key, Value)
    VALUES ('ProtocolVersion', $1);

INSERT INTO Meta(Key, Value)
    VALUES ('DatabaseVersion', $1);