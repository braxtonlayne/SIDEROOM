-- D-719 (SEC-AUD-225): a language is stored in its short form. A three-letter ISO 639-2 code that has a
-- two-letter ISO 639-1 code becomes it (`eng` -> `en`, `ger` and `deu` -> `de`, `fin` -> `fi`); a code without one
-- stays (`fil`) and the rest of a tag is kept (`por-BR` -> `pt-BR`).
--
-- Rewritten: `video_titles.languages` and `video_titles.subtitle_languages` (element by element, order kept, a
-- duplicate the rewrite makes removed), the `language_tag` of the audio and subtitle tracks, and the `language` of
-- the trailers. Left alone: `video_*_tracks.language` (the code as probed, a record of the file), file names,
-- storage keys and playlist paths. Only rows that hold a three-letter code are touched; `updated_at` does not move
-- (this is not an edit by staff). The search index holds no languages, so there is nothing to reindex.
--
-- Idempotent: a second run finds no three-letter code and changes nothing. The pairs are the same list as
-- `sr_core::language::LANGUAGE_PAIRS` (the test `the_sql_pairs_and_the_rust_table_are_the_same_list`).

DROP TABLE IF EXISTS sr_lang_pairs;
CREATE TEMP TABLE sr_lang_pairs (long TEXT PRIMARY KEY, short TEXT NOT NULL);
INSERT INTO sr_lang_pairs (long, short) VALUES
        ('aar','aa'), ('abk','ab'), ('afr','af'), ('aka','ak'), ('alb','sq'), ('amh','am'),
        ('ara','ar'), ('arg','an'), ('arm','hy'), ('asm','as'), ('ava','av'), ('ave','ae'),
        ('aym','ay'), ('aze','az'), ('bak','ba'), ('bam','bm'), ('baq','eu'), ('bel','be'),
        ('ben','bn'), ('bih','bh'), ('bis','bi'), ('bod','bo'), ('bos','bs'), ('bre','br'),
        ('bul','bg'), ('bur','my'), ('cat','ca'), ('ces','cs'), ('cha','ch'), ('che','ce'),
        ('chi','zh'), ('chu','cu'), ('chv','cv'), ('cor','kw'), ('cos','co'), ('cre','cr'),
        ('cym','cy'), ('cze','cs'), ('dan','da'), ('deu','de'), ('div','dv'), ('dut','nl'),
        ('dzo','dz'), ('ell','el'), ('eng','en'), ('epo','eo'), ('est','et'), ('eus','eu'),
        ('ewe','ee'), ('fao','fo'), ('fas','fa'), ('fij','fj'), ('fin','fi'), ('fra','fr'),
        ('fre','fr'), ('fry','fy'), ('ful','ff'), ('geo','ka'), ('ger','de'), ('gla','gd'),
        ('gle','ga'), ('glg','gl'), ('glv','gv'), ('gre','el'), ('grn','gn'), ('guj','gu'),
        ('hat','ht'), ('hau','ha'), ('heb','he'), ('her','hz'), ('hin','hi'), ('hmo','ho'),
        ('hrv','hr'), ('hun','hu'), ('hye','hy'), ('ibo','ig'), ('ice','is'), ('ido','io'),
        ('iii','ii'), ('iku','iu'), ('ile','ie'), ('ina','ia'), ('ind','id'), ('ipk','ik'),
        ('isl','is'), ('ita','it'), ('jav','jv'), ('jpn','ja'), ('kal','kl'), ('kan','kn'),
        ('kas','ks'), ('kat','ka'), ('kau','kr'), ('kaz','kk'), ('khm','km'), ('kik','ki'),
        ('kin','rw'), ('kir','ky'), ('kom','kv'), ('kon','kg'), ('kor','ko'), ('kua','kj'),
        ('kur','ku'), ('lao','lo'), ('lat','la'), ('lav','lv'), ('lim','li'), ('lin','ln'),
        ('lit','lt'), ('ltz','lb'), ('lub','lu'), ('lug','lg'), ('mac','mk'), ('mah','mh'),
        ('mal','ml'), ('mao','mi'), ('mar','mr'), ('may','ms'), ('mkd','mk'), ('mlg','mg'),
        ('mlt','mt'), ('mon','mn'), ('mri','mi'), ('msa','ms'), ('mya','my'), ('nau','na'),
        ('nav','nv'), ('nbl','nr'), ('nde','nd'), ('ndo','ng'), ('nep','ne'), ('nld','nl'),
        ('nno','nn'), ('nob','nb'), ('nor','no'), ('nya','ny'), ('oci','oc'), ('oji','oj'),
        ('ori','or'), ('orm','om'), ('oss','os'), ('pan','pa'), ('per','fa'), ('pli','pi'),
        ('pol','pl'), ('por','pt'), ('pus','ps'), ('que','qu'), ('roh','rm'), ('ron','ro'),
        ('rum','ro'), ('run','rn'), ('rus','ru'), ('sag','sg'), ('san','sa'), ('sin','si'),
        ('slk','sk'), ('slo','sk'), ('slv','sl'), ('sme','se'), ('smo','sm'), ('sna','sn'),
        ('snd','sd'), ('som','so'), ('sot','st'), ('spa','es'), ('sqi','sq'), ('srd','sc'),
        ('srp','sr'), ('ssw','ss'), ('sun','su'), ('swa','sw'), ('swe','sv'), ('tah','ty'),
        ('tam','ta'), ('tat','tt'), ('tel','te'), ('tgk','tg'), ('tgl','tl'), ('tha','th'),
        ('tib','bo'), ('tir','ti'), ('ton','to'), ('tsn','tn'), ('tso','ts'), ('tuk','tk'),
        ('tur','tr'), ('twi','tw'), ('uig','ug'), ('ukr','uk'), ('urd','ur'), ('uzb','uz'),
        ('ven','ve'), ('vie','vi'), ('vol','vo'), ('wel','cy'), ('wln','wa'), ('wol','wo'),
        ('xho','xh'), ('yid','yi'), ('yor','yo'), ('zha','za'), ('zho','zh'), ('zul','zu');

-- A tag with its primary subtag (before the first `-`, compared in lower case) in short form; a tag whose
-- primary subtag is not in the list comes back as it is.
CREATE FUNCTION pg_temp.sr_short_tag(tag TEXT) RETURNS TEXT
    LANGUAGE sql STABLE AS
$$
    SELECT coalesce(
        (SELECT p.short || substr(tag, length(split_part(tag, '-', 1)) + 1)
           FROM sr_lang_pairs p WHERE p.long = lower(split_part(tag, '-', 1))),
        tag)
$$;

-- A list with every element in short form: order kept (first place of each value), duplicates removed.
CREATE FUNCTION pg_temp.sr_short_list(tags TEXT[]) RETURNS TEXT[]
    LANGUAGE sql STABLE AS
$$
    SELECT coalesce(array_agg(v ORDER BY first_ord), '{}')
      FROM (SELECT pg_temp.sr_short_tag(e) AS v, min(ord) AS first_ord
              FROM unnest(tags) WITH ORDINALITY AS u(e, ord)
             GROUP BY 1) s
$$;

UPDATE video_titles
   SET languages = pg_temp.sr_short_list(languages)
 WHERE EXISTS (SELECT 1 FROM unnest(languages) e WHERE pg_temp.sr_short_tag(e) <> e);

UPDATE video_titles
   SET subtitle_languages = pg_temp.sr_short_list(subtitle_languages)
 WHERE EXISTS (SELECT 1 FROM unnest(subtitle_languages) e WHERE pg_temp.sr_short_tag(e) <> e);

UPDATE video_audio_tracks
   SET language_tag = pg_temp.sr_short_tag(language_tag)
 WHERE language_tag IS NOT NULL AND pg_temp.sr_short_tag(language_tag) <> language_tag;

UPDATE video_subtitle_tracks
   SET language_tag = pg_temp.sr_short_tag(language_tag)
 WHERE language_tag IS NOT NULL AND pg_temp.sr_short_tag(language_tag) <> language_tag;

UPDATE video_trailers
   SET language = pg_temp.sr_short_tag(language)
 WHERE language IS NOT NULL AND pg_temp.sr_short_tag(language) <> language;

DROP FUNCTION pg_temp.sr_short_list(TEXT[]);
DROP FUNCTION pg_temp.sr_short_tag(TEXT);
DROP TABLE sr_lang_pairs;
