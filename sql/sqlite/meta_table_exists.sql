SELECT EXISTS (
   SELECT 1 FROM sqlite_master
   WHERE type = 'table' AND lower(name) = 'meta'
);
