# Launch Library 2 Supplementary Docs

This document is a handy short-form reference for the Launch Library 2 API, containing the
relevant endpoint descriptions for the Launch TUI client. Use this as a reference to help
with design decisions, and then verify those decisions with the official documentation.

**Full documentation**: [Launch Library 2 Docs](https://ll.thespacedevs.com/docs/)

Base URL: https://ll.thespacedevs.com/2.3.0/

## Endpoints

### Launches
GET - /launches/

#### Purpose
Returns a list of launches with varying detail depending on the selected mode.

#### Parameters
**Modes**:
Levels of detail in the response - list, normal, detailed
Example - /launches/?mode=list

**Available Filters**:
A comma separated list of available filter parameters:
```
  agency_launch_attempt_count, agency_launch_attempt_count__gt, agency_launch_attempt_count__gte, agency_launch_attempt_count__lt, agency_launch_attempt_count__lte, agency_launch_attempt_count_year, agency_launch_attempt_count_year__gt, agency_launch_attempt_count_year__gte, agency_launch_attempt_count_year__lt, agency_launch_attempt_count_year__lte, day, id, include_suborbital, is_crewed, last_updated__gte, last_updated__lte, launch_designator, launcher_config__id, location__ids, location_launch_attempt_count, location_launch_attempt_count__gt, location_launch_attempt_count__gte, location_launch_attempt_count__lt, location_launch_attempt_count__lte, location_launch_attempt_count_year, location_launch_attempt_count_year__gt, location_launch_attempt_count_year__gte, location_launch_attempt_count_year__lt, location_launch_attempt_count_year__lte, lsp__id, lsp__name, mission__agency__ids, mission__orbit__celestial_body__id, mission__orbit__name, mission__orbit__name__icontains, month, name, net__gt, net__gte, net__lt, net__lte, orbital_launch_attempt_count, orbital_launch_attempt_count__gt, orbital_launch_attempt_count__gte, orbital_launch_attempt_count__lt, orbital_launch_attempt_count__lte, orbital_launch_attempt_count_year, orbital_launch_attempt_count_year__gt, orbital_launch_attempt_count_year__gte, orbital_launch_attempt_count_year__lt, orbital_launch_attempt_count_year__lte, pad, pad__location, pad__location__celestial_body__id, pad_launch_attempt_count, pad_launch_attempt_count__gt, pad_launch_attempt_count__gte, pad_launch_attempt_count__lt, pad_launch_attempt_count__lte, pad_launch_attempt_count_year, pad_launch_attempt_count_year__gt, pad_launch_attempt_count_year__gte, pad_launch_attempt_count_year__lt, pad_launch_attempt_count_year__lte, program, related_lsp__id, related_lsp__name, rocket__configuration__full_name, rocket__configuration__full_name__icontains, rocket__configuration__id, rocket__configuration__manufacturer__name, rocket__configuration__manufacturer__name__icontains, rocket__configuration__name, rocket__spacecraftflight__spacecraft__id, rocket__spacecraftflight__spacecraft__name, rocket__spacecraftflight__spacecraft__name__icontains, serial_number, slug, spacecraft_config__ids, status, status__ids, video_url, window_end__gt, window_end__gte, window_end__lt, window_end__lte, window_start__gt, window_start__gte, window_start__lt, window_start__lte, year
```

Example - /launches/?pad__location=13

**Available Search Fields**:
Fields searched:
```
  launch_designator, launch_service_provider__name, mission__name, name, pad__location__name, pad__name, rocket__configuration__manufacturer__abbrev, rocket__configuration__manufacturer__name, rocket__configuration__name, rocket__spacecraftflight__spacecraft__name
```

Example - /launches/?search=Starlink

**Ordering**:
Fields - id, last_updated, name, net

Example - /launches/?ordering=-last_updated

**Number of results**:
Use limit to control the number of objects in the response (max 100)

Example - /launches/?limit=2

#### Return value
status code: 200

Example return value:
```
{
  "count": 123,
  "next": "http://api.example.org/accounts/?offset=400&limit=100",
  "previous": "http://api.example.org/accounts/?offset=200&limit=100",
  "results": [
    {
      "id": "3fa85f64-5717-4562-b3fc-2c963f66afa6",
      "url": "string",
      "name": "string",
      "response_mode": "list",
      "slug": "85otNHAnEB8PuGxRHtaHd41LjyigMfgcyn0DabLDddYlJ8IcsMyvZVSZYdeL",
      "launch_designator": "string",
      "status": {
        "id": 2147483647,
        "name": "string",
        "abbrev": "string",
        "description": "string"
      },
      "last_updated": "2026-02-24T22:32:08.710Z",
      "net": "2026-02-24T22:32:08.710Z",
      "net_precision": {
        "id": 2147483647,
        "name": "string",
        "abbrev": "string",
        "description": "string"
      },
      "window_end": "2026-02-24T22:32:08.710Z",
      "window_start": "2026-02-24T22:32:08.710Z",
      "image": {
        "id": 0,
        "name": "string",
        "image_url": "string",
        "thumbnail_url": "string",
        "credit": "string",
        "license": {
          "id": 0,
          "name": "string",
          "priority": 2147483647,
          "link": "string"
        },
        "single_use": true,
        "variants": [
          {
            "id": 0,
            "type": {
              "id": 0,
              "name": "string"
            },
            "image_url": "string"
          }
        ]
      },
      "infographic": "string"
    },
    {
      "id": "3fa85f64-5717-4562-b3fc-2c963f66afa6",
      "url": "string",
      "name": "string",
      "response_mode": "normal",
      "slug": "dpYmLQG-t",
      "launch_designator": "string",
      "status": {
        "id": 2147483647,
        "name": "string",
        "abbrev": "string",
        "description": "string"
      },
      "last_updated": "2026-02-24T22:32:08.710Z",
      "net": "2026-02-24T22:32:08.710Z",
      "net_precision": {
        "id": 2147483647,
        "name": "string",
        "abbrev": "string",
        "description": "string"
      },
      "window_end": "2026-02-24T22:32:08.710Z",
      "window_start": "2026-02-24T22:32:08.710Z",
      "image": {
        "id": 0,
        "name": "string",
        "image_url": "string",
        "thumbnail_url": "string",
        "credit": "string",
        "license": {
          "id": 0,
          "name": "string",
          "priority": 2147483647,
          "link": "string"
        },
        "single_use": true,
        "variants": [
          {
            "id": 0,
            "type": {
              "id": 0,
              "name": "string"
            },
            "image_url": "string"
          }
        ]
      },
      "infographic": "string",
      "probability": 2147483647,
      "weather_concerns": "string",
      "failreason": "string",
      "hashtag": "string",
      "launch_service_provider": {
        "response_mode": "list",
        "id": 0,
        "url": "string",
        "name": "string",
        "abbrev": "string",
        "type": {
          "id": 2147483647,
          "name": "string"
        }
      },
      "rocket": {
        "id": 0,
        "configuration": {
          "response_mode": "list",
          "id": 0,
          "url": "string",
          "name": "string",
          "families": [
            {
              "response_mode": "list",
              "id": 0,
              "name": "string"
            }
          ],
          "full_name": "string",
          "variant": "string"
        }
      },
      "mission": {
        "id": 0,
        "name": "string",
        "type": "string",
        "description": "string",
        "image": {
          "id": 0,
          "name": "string",
          "image_url": "string",
          "thumbnail_url": "string",
          "credit": "string",
          "license": {
            "id": 0,
            "name": "string",
            "priority": 2147483647,
            "link": "string"
          },
          "single_use": true,
          "variants": [
            {
              "id": 0,
              "type": {
                "id": 0,
                "name": "string"
              },
              "image_url": "string"
            }
          ]
        },
        "orbit": {
          "id": 0,
          "name": "string",
          "abbrev": "string",
          "celestial_body": {
            "response_mode": "list",
            "id": 0,
            "name": "string"
          }
        },
        "agencies": [
          {
            "response_mode": "normal",
            "id": 0,
            "url": "string",
            "name": "string",
            "abbrev": "string",
            "type": {
              "id": 2147483647,
              "name": "string"
            },
            "featured": true,
            "country": [
              {
                "id": 0,
                "name": "string",
                "alpha_2_code": "st",
                "alpha_3_code": "str",
                "nationality_name": "string",
                "nationality_name_composed": "string"
              }
            ],
            "description": "string",
            "administrator": "string",
            "founding_year": 2147483647,
            "launchers": "string",
            "spacecraft": "string",
            "parent": "string",
            "image": {
              "id": 0,
              "name": "string",
              "image_url": "string",
              "thumbnail_url": "string",
              "credit": "string",
              "license": {
                "id": 0,
                "name": "string",
                "priority": 2147483647,
                "link": "string"
              },
              "single_use": true,
              "variants": [
                {
                  "id": 0,
                  "type": {
                    "id": 0,
                    "name": "string"
                  },
                  "image_url": "string"
                }
              ]
            },
            "logo": {
              "id": 0,
              "name": "string",
              "image_url": "string",
              "thumbnail_url": "string",
              "credit": "string",
              "license": {
                "id": 0,
                "name": "string",
                "priority": 2147483647,
                "link": "string"
              },
              "single_use": true,
              "variants": [
                {
                  "id": 0,
                  "type": {
                    "id": 0,
                    "name": "string"
                  },
                  "image_url": "string"
                }
              ]
            },
            "social_logo": {
              "id": 0,
              "name": "string",
              "image_url": "string",
              "thumbnail_url": "string",
              "credit": "string",
              "license": {
                "id": 0,
                "name": "string",
                "priority": 2147483647,
                "link": "string"
              },
              "single_use": true,
              "variants": [
                {
                  "id": 0,
                  "type": {
                    "id": 0,
                    "name": "string"
                  },
                  "image_url": "string"
                }
              ]
            },
            "total_launch_count": 2147483647,
            "consecutive_successful_launches": 2147483647,
            "successful_launches": 2147483647,
            "failed_launches": 2147483647,
            "pending_launches": 2147483647,
            "consecutive_successful_landings": 2147483647,
            "successful_landings": 2147483647,
            "failed_landings": 2147483647,
            "attempted_landings": 2147483647,
            "successful_landings_spacecraft": 2147483647,
            "failed_landings_spacecraft": 2147483647,
            "attempted_landings_spacecraft": 2147483647,
            "successful_landings_payload": 2147483647,
            "failed_landings_payload": 2147483647,
            "attempted_landings_payload": 2147483647,
            "info_url": "string",
            "wiki_url": "string",
            "social_media_links": [
              {
                "id": 0,
                "social_media": {
                  "id": 0,
                  "name": "string",
                  "url": "string",
                  "logo": {
                    "id": 0,
                    "name": "string",
                    "image_url": "string",
                    "thumbnail_url": "string",
                    "credit": "string",
                    "license": {
                      "id": 0,
                      "name": "string",
                      "priority": 2147483647,
                      "link": "string"
                    },
                    "single_use": true,
                    "variants": [
                      {
                        "id": 0,
                        "type": {
                          "id": 0,
                          "name": "string"
                        },
                        "image_url": "string"
                      }
                    ]
                  }
                },
                "url": "string"
              }
            ]
          }
        ],
        "info_urls": [
          {
            "priority": 2147483647,
            "source": "string",
            "title": "string",
            "description": "string",
            "feature_image": "string",
            "url": "string",
            "type": {
              "id": 0,
              "name": "string"
            },
            "language": {
              "id": 0,
              "name": "string",
              "code": "string"
            }
          }
        ],
        "vid_urls": [
          {
            "priority": 2147483647,
            "source": "string",
            "publisher": "string",
            "title": "string",
            "description": "string",
            "feature_image": "string",
            "url": "string",
            "type": {
              "id": 0,
              "name": "string"
            },
            "language": {
              "id": 0,
              "name": "string",
              "code": "string"
            },
            "start_time": "2026-02-24T22:32:08.710Z",
            "end_time": "2026-02-24T22:32:08.710Z",
            "live": true
          }
        ]
      },
      "pad": {
        "id": 0,
        "url": "string",
        "active": true,
        "agencies": [
          {
            "response_mode": "normal",
            "id": 0,
            "url": "string",
            "name": "string",
            "abbrev": "string",
            "type": {
              "id": 2147483647,
              "name": "string"
            },
            "featured": true,
            "country": [
              {
                "id": 0,
                "name": "string",
                "alpha_2_code": "st",
                "alpha_3_code": "str",
                "nationality_name": "string",
                "nationality_name_composed": "string"
              }
            ],
            "description": "string",
            "administrator": "string",
            "founding_year": 2147483647,
            "launchers": "string",
            "spacecraft": "string",
            "parent": "string",
            "image": {
              "id": 0,
              "name": "string",
              "image_url": "string",
              "thumbnail_url": "string",
              "credit": "string",
              "license": {
                "id": 0,
                "name": "string",
                "priority": 2147483647,
                "link": "string"
              },
              "single_use": true,
              "variants": [
                {
                  "id": 0,
                  "type": {
                    "id": 0,
                    "name": "string"
                  },
                  "image_url": "string"
                }
              ]
            },
            "logo": {
              "id": 0,
              "name": "string",
              "image_url": "string",
              "thumbnail_url": "string",
              "credit": "string",
              "license": {
                "id": 0,
                "name": "string",
                "priority": 2147483647,
                "link": "string"
              },
              "single_use": true,
              "variants": [
                {
                  "id": 0,
                  "type": {
                    "id": 0,
                    "name": "string"
                  },
                  "image_url": "string"
                }
              ]
            },
            "social_logo": {
              "id": 0,
              "name": "string",
              "image_url": "string",
              "thumbnail_url": "string",
              "credit": "string",
              "license": {
                "id": 0,
                "name": "string",
                "priority": 2147483647,
                "link": "string"
              },
              "single_use": true,
              "variants": [
                {
                  "id": 0,
                  "type": {
                    "id": 0,
                    "name": "string"
                  },
                  "image_url": "string"
                }
              ]
            }
          }
        ],
        "name": "string",
        "image": {
          "id": 0,
          "name": "string",
          "image_url": "string",
          "thumbnail_url": "string",
          "credit": "string",
          "license": {
            "id": 0,
            "name": "string",
            "priority": 2147483647,
            "link": "string"
          },
          "single_use": true,
          "variants": [
            {
              "id": 0,
              "type": {
                "id": 0,
                "name": "string"
              },
              "image_url": "string"
            }
          ]
        },
        "description": "string",
        "info_url": "string",
        "wiki_url": "string",
        "map_url": "string",
        "latitude": 0,
        "longitude": 0,
        "country": {
          "id": 0,
          "name": "string",
          "alpha_2_code": "st",
          "alpha_3_code": "str",
          "nationality_name": "string",
          "nationality_name_composed": "string"
        },
        "map_image": "string",
        "total_launch_count": 2147483647,
        "orbital_launch_attempt_count": 2147483647,
        "fastest_turnaround": "string",
        "location": {
          "response_mode": "normal",
          "id": 0,
          "url": "string",
          "name": "string",
          "celestial_body": {
            "response_mode": "normal",
            "id": 0,
            "name": "string",
            "type": {
              "id": 2147483647,
              "name": "string"
            },
            "diameter": 0,
            "mass": 0,
            "gravity": 0,
            "length_of_day": "string",
            "atmosphere": true,
            "image": {
              "id": 0,
              "name": "string",
              "image_url": "string",
              "thumbnail_url": "string",
              "credit": "string",
              "license": {
                "id": 0,
                "name": "string",
                "priority": 2147483647,
                "link": "string"
              },
              "single_use": true,
              "variants": [
                {
                  "id": 0,
                  "type": {
                    "id": 0,
                    "name": "string"
                  },
                  "image_url": "string"
                }
              ]
            },
            "description": "string",
            "wiki_url": "string",
            "total_attempted_launches": 2147483647,
            "successful_launches": 2147483647,
            "failed_launches": 2147483647,
            "total_attempted_landings": 2147483647,
            "successful_landings": 2147483647,
            "failed_landings": 2147483647
          },
          "active": true,
          "country": {
            "id": 0,
            "name": "string",
            "alpha_2_code": "st",
            "alpha_3_code": "str",
            "nationality_name": "string",
            "nationality_name_composed": "string"
          },
          "description": "string",
          "image": {
            "id": 0,
            "name": "string",
            "image_url": "string",
            "thumbnail_url": "string",
            "credit": "string",
            "license": {
              "id": 0,
              "name": "string",
              "priority": 2147483647,
              "link": "string"
            },
            "single_use": true,
            "variants": [
              {
                "id": 0,
                "type": {
                  "id": 0,
                  "name": "string"
                },
                "image_url": "string"
              }
            ]
          },
          "map_image": "string",
          "longitude": 0,
          "latitude": 0,
          "timezone_name": "string",
          "total_launch_count": 2147483647,
          "total_landing_count": 2147483647
        }
      },
      "webcast_live": true,
      "program": [
        {
          "response_mode": "normal",
          "id": 0,
          "url": "string",
          "name": "string",
          "image": {
            "id": 0,
            "name": "string",
            "image_url": "string",
            "thumbnail_url": "string",
            "credit": "string",
            "license": {
              "id": 0,
              "name": "string",
              "priority": 2147483647,
              "link": "string"
            },
            "single_use": true,
            "variants": [
              {
                "id": 0,
                "type": {
                  "id": 0,
                  "name": "string"
                },
                "image_url": "string"
              }
            ]
          },
          "info_url": "string",
          "wiki_url": "string",
          "description": "string",
          "agencies": [
            {
              "response_mode": "list",
              "id": 0,
              "url": "string",
              "name": "string",
              "abbrev": "string",
              "type": {
                "id": 2147483647,
                "name": "string"
              }
            }
          ],
          "start_date": "2026-02-24T22:32:08.710Z",
          "end_date": "2026-02-24T22:32:08.710Z",
          "mission_patches": [
            {
              "id": 0,
              "name": "string",
              "priority": 2147483647,
              "image_url": "string",
              "agency": {
                "response_mode": "list",
                "id": 0,
                "url": "string",
                "name": "string",
                "abbrev": "string",
                "type": {
                  "id": 2147483647,
                  "name": "string"
                }
              },
              "response_mode": "normal"
            }
          ],
          "type": {
            "id": 2147483647,
            "name": "string"
          }
        }
      ],
      "orbital_launch_attempt_count": 2147483647,
      "location_launch_attempt_count": 2147483647,
      "pad_launch_attempt_count": 2147483647,
      "agency_launch_attempt_count": 2147483647,
      "orbital_launch_attempt_count_year": 2147483647,
      "location_launch_attempt_count_year": 2147483647,
      "pad_launch_attempt_count_year": 2147483647,
      "agency_launch_attempt_count_year": 2147483647
    }
  ]
}
```

## Launch Detail
GET /launch/{launch_id}/

#### Purpose
Gets the details of a specific launch by id. Useful for getting more info about a specific launch

#### Return value
status code: 200 | 404

Example return value:
```
  {
    "id": "e3df2ecd-c239-472f-95e4-2b89b4f75800",
    "url": "https://ll.thespacedevs.com/2.3.0/launches/e3df2ecd-c239-472f-95e4-2b89b4f75800/?format=json",
    "name": "Sputnik 8K74PS | Sputnik 1",
    "response_mode": "detailed",
    "slug": "sputnik-8k74ps-sputnik-1",
    "launch_designator": "1957-001",
    "status": {
      "id": 3,
      "name": "Launch Successful",
      "abbrev": "Success",
      "description": "The launch vehicle successfully inserted its payload(s) into the target orbit(s)."
    },
    "last_updated": "2024-03-17T19:17:35Z",
    "net": "1957-10-04T19:28:34Z",
    "net_precision": null,
    "window_end": "1957-10-04T19:28:34Z",
    "window_start": "1957-10-04T19:28:34Z",
    "image": {
      "id": 1844,
      "name": "[AUTO] Sputnik 8K74PS - image",
      "image_url": "https://thespacedevs-prod.nyc3.digitaloceanspaces.com/media/images/sputnik_8k74ps_image_20210830185541.jpg",
      "thumbnail_url": "https://thespacedevs-prod.nyc3.digitaloceanspaces.com/media/images/255bauto255d__image_thumbnail_20240305193923.jpeg",
      "credit": null,
      "license": {
        "id": 1,
        "name": "Unknown",
        "priority": 9,
        "link": null
      },
      "single_use": true,
      "variants": []
    },
    "infographic": null,
    "probability": null,
    "weather_concerns": null,
    "failreason": "",
    "hashtag": null,
    "launch_service_provider": {
      "response_mode": "normal",
      "id": 66,
      "url": "https://ll.thespacedevs.com/2.3.0/agencies/66/?format=json",
      "name": "Soviet Space Program",
      "abbrev": "CCCP",
      "type": {
        "id": 1,
        "name": "Government"
      },
      "featured": false,
      "country": [
        {
          "id": 5,
          "name": "Russia",
          "alpha_2_code": "RU",
          "alpha_3_code": "RUS",
          "nationality_name": "Russian",
          "nationality_name_composed": "Russo"
        }
      ],
      "description": "The Soviet space program, was the national space program of the Union of Soviet Socialist Republics (USSR) actived from 1930s until disintegration of the Soviet Union in 1991.\r\n\r\nThe Soviet Union's space program was mainly based on the cosmonautic exploration of space and the development of the expandable launch vehicles, which had been split between many design bureaus competing against each other. Over its 60-years of history, the Russian program was responsible for a number of pioneering feats and accomplishments in the human space flight, including the first intercontinental ballistic missile (R-7), first satellite (Sputnik 1), first animal in Earth orbit (the dog Laika on Sputnik 2), first human in space and Earth orbit (cosmonaut Yuri Gagarin on Vostok 1), first woman in space and Earth orbit (cosmonaut Valentina Tereshkova on Vostok 6), first spacewalk (cosmonaut Alexei Leonov on Voskhod 2), first Moon impact (Luna 2), first image of the far side of the Moon (Luna 3) and unmanned lunar soft landing (Luna 9), first space rover (Lunokhod 1), first sample of lunar soil automatically extracted and brought to Earth (Luna 16), and first space station (Salyut 1). Further notable records included the first interplanetary probes: Venera 1 and Mars 1 to fly by Venus and Mars, respectively, Venera 3 and Mars 2 to impact the respective planet surface, and Venera 7 and Mars 3 to make soft landings on these planets.",
      "administrator": null,
      "founding_year": 1931,
      "launchers": "",
      "spacecraft": "",
      "parent": null,
      "image": {
        "id": 28,
        "name": "[AUTO] Soviet Space Program - image",
        "image_url": "https://thespacedevs-prod.nyc3.digitaloceanspaces.com/media/images/soviet2520space2520program_image_20191229081306.jpeg",
        "thumbnail_url": "https://thespacedevs-prod.nyc3.digitaloceanspaces.com/media/images/255bauto255d__image_thumbnail_20240305184701.jpeg",
        "credit": null,
        "license": {
          "id": 1,
          "name": "Unknown",
          "priority": 9,
          "link": null
        },
        "single_use": true,
        "variants": []
      },
      "logo": {
        "id": 182,
        "name": "Soviet Space Program logo",
        "image_url": "https://thespacedevs-prod.nyc3.digitaloceanspaces.com/media/images/soviet2520space2520program_logo_20191229081307.png",
        "thumbnail_url": "https://thespacedevs-prod.nyc3.digitaloceanspaces.com/media/images/255bauto255d__image_thumbnail_20240305185129.png",
        "credit": "USSR",
        "license": {
          "id": 1,
          "name": "Unknown",
          "priority": 9,
          "link": null
        },
        "single_use": true,
        "variants": []
      },
      "social_logo": {
        "id": 92,
        "name": "Soviet Space Program social logo",
        "image_url": "https://thespacedevs-prod.nyc3.digitaloceanspaces.com/media/images/soviet_space_pr_image_20251017113333.png",
        "thumbnail_url": "https://thespacedevs-prod.nyc3.digitaloceanspaces.com/media/images/soviet_space_pr_image_thumbnail_20251017113333.png",
        "credit": "USSR",
        "license": {
          "id": 1,
          "name": "Unknown",
          "priority": 9,
          "link": null
        },
        "single_use": true,
        "variants": []
      },
      "total_launch_count": 2456,
      "consecutive_successful_launches": 17,
      "successful_launches": 2288,
      "failed_launches": 168,
      "pending_launches": 0,
      "consecutive_successful_landings": 0,
      "successful_landings": 0,
      "failed_landings": 0,
      "attempted_landings": 0,
      "successful_landings_spacecraft": 0,
      "failed_landings_spacecraft": 0,
      "attempted_landings_spacecraft": 0,
      "successful_landings_payload": 3,
      "failed_landings_payload": 0,
      "attempted_landings_payload": 3,
      "info_url": null,
      "wiki_url": "https://en.wikipedia.org/wiki/Soviet_space_program",
      "social_media_links": []
    },
    "rocket": {
      "id": 3003,
      "configuration": {
        "response_mode": "detailed",
        "id": 468,
        "url": "https://ll.thespacedevs.com/2.3.0/launcher_configurations/468/?format=json",
        "name": "Sputnik 8K74PS",
        "families": [
          {
            "response_mode": "detailed",
            "id": 154,
            "name": "Sputnik",
            "manufacturer": [
              {
                "response_mode": "normal",
                "id": 1000,
                "url": "https://ll.thespacedevs.com/2.3.0/agencies/1000/?format=json",
                "name": "Energia",
                "abbrev": "OKB-1",
                "type": {
                  "id": 1,
                  "name": "Government"
                },
                "featured": false,
                "country": [
                  {
                    "id": 5,
                    "name": "Russia",
                    "alpha_2_code": "RU",
                    "alpha_3_code": "RUS",
                    "nationality_name": "Russian",
                    "nationality_name_composed": "Russo"
                  }
                ],
                "description": null,
                "administrator": null,
                "founding_year": 1946,
                "launchers": "",
                "spacecraft": "",
                "parent": null,
                "image": null,
                "logo": null,
                "social_logo": null,
                "total_launch_count": 0,
                "consecutive_successful_launches": 0,
                "successful_launches": 0,
                "failed_launches": 0,
                "pending_launches": 0,
                "consecutive_successful_landings": 0,
                "successful_landings": 0,
                "failed_landings": 0,
                "attempted_landings": 0,
                "successful_landings_spacecraft": 0,
                "failed_landings_spacecraft": 0,
                "attempted_landings_spacecraft": 0,
                "successful_landings_payload": 0,
                "failed_landings_payload": 0,
                "attempted_landings_payload": 0,
                "info_url": null,
                "wiki_url": "https://en.wikipedia.org/wiki/Energia_(corporation)",
                "social_media_links": []
              }
            ],
            "parent": null,
            "description": "",
            "active": false,
            "maiden_flight": "1957-10-04",
            "total_launch_count": 6,
            "consecutive_successful_launches": 3,
            "successful_launches": 5,
            "failed_launches": 1,
            "pending_launches": 0,
            "attempted_landings": 0,
            "successful_landings": 0,
            "failed_landings": 0,
            "consecutive_successful_landings": 0
          }
        ],
        "full_name": "Sputnik 8K74PS",
        "variant": "8K74PS",
        "active": false,
        "is_placeholder": false,
        "manufacturer": {
          "response_mode": "normal",
          "id": 1000,
          "url": "https://ll.thespacedevs.com/2.3.0/agencies/1000/?format=json",
          "name": "Energia",
          "abbrev": "OKB-1",
          "type": {
            "id": 1,
            "name": "Government"
          },
          "featured": false,
          "country": [
            {
              "id": 5,
              "name": "Russia",
              "alpha_2_code": "RU",
              "alpha_3_code": "RUS",
              "nationality_name": "Russian",
              "nationality_name_composed": "Russo"
            }
          ],
          "description": null,
          "administrator": null,
          "founding_year": 1946,
          "launchers": "",
          "spacecraft": "",
          "parent": null,
          "image": null,
          "logo": null,
          "social_logo": null,
          "total_launch_count": 0,
          "consecutive_successful_launches": 0,
          "successful_launches": 0,
          "failed_launches": 0,
          "pending_launches": 0,
          "consecutive_successful_landings": 0,
          "successful_landings": 0,
          "failed_landings": 0,
          "attempted_landings": 0,
          "successful_landings_spacecraft": 0,
          "failed_landings_spacecraft": 0,
          "attempted_landings_spacecraft": 0,
          "successful_landings_payload": 0,
          "failed_landings_payload": 0,
          "attempted_landings_payload": 0,
          "info_url": null,
          "wiki_url": "https://en.wikipedia.org/wiki/Energia_(corporation)",
          "social_media_links": []
        },
        "program": [],
        "reusable": false,
        "image": {
          "id": 1844,
          "name": "[AUTO] Sputnik 8K74PS - image",
          "image_url": "https://thespacedevs-prod.nyc3.digitaloceanspaces.com/media/images/sputnik_8k74ps_image_20210830185541.jpg",
          "thumbnail_url": "https://thespacedevs-prod.nyc3.digitaloceanspaces.com/media/images/255bauto255d__image_thumbnail_20240305193923.jpeg",
          "credit": null,
          "license": {
            "id": 1,
            "name": "Unknown",
            "priority": 9,
            "link": null
          },
          "single_use": true,
          "variants": []
        },
        "info_url": null,
        "wiki_url": null,
        "description": "An early Russian rocket designed by Sergei Korolev in the Soviet Union",
        "alias": "",
        "min_stage": 1,
        "max_stage": 1,
        "length": null,
        "diameter": null,
        "maiden_flight": "1957-10-04",
        "launch_cost": null,
        "launch_mass": null,
        "leo_capacity": null,
        "gto_capacity": null,
        "geo_capacity": null,
        "sso_capacity": null,
        "to_thrust": null,
        "apogee": null,
        "total_launch_count": 2,
        "consecutive_successful_launches": 2,
        "successful_launches": 2,
        "failed_launches": 0,
        "pending_launches": 0,
        "attempted_landings": 0,
        "successful_landings": 0,
        "failed_landings": 0,
        "consecutive_successful_landings": 0,
        "fastest_turnaround": "P29DT7H1M26S"
      },
      "launcher_stage": [],
      "spacecraft_stage": [],
      "payloads": [
        {
          "response_mode": "detailed",
          "id": 5,
          "url": "https://ll.thespacedevs.com/2.3.0/payload_flights/5/?format=json",
          "destination": "Orbit",
          "amount": 1,
          "payload": {
            "response_mode": "detailed",
            "id": 5,
            "name": "Sputnik 1",
            "type": {
              "id": 4,
              "name": "Technology Demonstrator"
            },
            "manufacturer": {
              "response_mode": "normal",
              "id": 1000,
              "url": "https://ll.thespacedevs.com/2.3.0/agencies/1000/?format=json",
              "name": "Energia",
              "abbrev": "OKB-1",
              "type": {
                "id": 1,
                "name": "Government"
              },
              "featured": false,
              "country": [
                {
                  "id": 5,
                  "name": "Russia",
                  "alpha_2_code": "RU",
                  "alpha_3_code": "RUS",
                  "nationality_name": "Russian",
                  "nationality_name_composed": "Russo"
                }
              ],
              "description": null,
              "administrator": null,
              "founding_year": 1946,
              "launchers": "",
              "spacecraft": "",
              "parent": null,
              "image": null,
              "logo": null,
              "social_logo": null,
              "total_launch_count": 0,
              "consecutive_successful_launches": 0,
              "successful_launches": 0,
              "failed_launches": 0,
              "pending_launches": 0,
              "consecutive_successful_landings": 0,
              "successful_landings": 0,
              "failed_landings": 0,
              "attempted_landings": 0,
              "successful_landings_spacecraft": 0,
              "failed_landings_spacecraft": 0,
              "attempted_landings_spacecraft": 0,
              "successful_landings_payload": 0,
              "failed_landings_payload": 0,
              "attempted_landings_payload": 0,
              "info_url": null,
              "wiki_url": "https://en.wikipedia.org/wiki/Energia_(corporation)",
              "social_media_links": []
            },
            "operator": {
              "response_mode": "normal",
              "id": 66,
              "url": "https://ll.thespacedevs.com/2.3.0/agencies/66/?format=json",
              "name": "Soviet Space Program",
              "abbrev": "CCCP",
              "type": {
                "id": 1,
                "name": "Government"
              },
              "featured": false,
              "country": [
                {
                  "id": 5,
                  "name": "Russia",
                  "alpha_2_code": "RU",
                  "alpha_3_code": "RUS",
                  "nationality_name": "Russian",
                  "nationality_name_composed": "Russo"
                }
              ],
              "description": "The Soviet space program, was the national space program of the Union of Soviet Socialist Republics (USSR) actived from 1930s until disintegration of the Soviet Union in 1991.\r\n\r\nThe Soviet Union's space program was mainly based on the cosmonautic exploration of space and the development of the expandable launch vehicles, which had been split between many design bureaus competing against each other. Over its 60-years of history, the Russian program was responsible for a number of pioneering feats and accomplishments in the human space flight, including the first intercontinental ballistic missile (R-7), first satellite (Sputnik 1), first animal in Earth orbit (the dog Laika on Sputnik 2), first human in space and Earth orbit (cosmonaut Yuri Gagarin on Vostok 1), first woman in space and Earth orbit (cosmonaut Valentina Tereshkova on Vostok 6), first spacewalk (cosmonaut Alexei Leonov on Voskhod 2), first Moon impact (Luna 2), first image of the far side of the Moon (Luna 3) and unmanned lunar soft landing (Luna 9), first space rover (Lunokhod 1), first sample of lunar soil automatically extracted and brought to Earth (Luna 16), and first space station (Salyut 1). Further notable records included the first interplanetary probes: Venera 1 and Mars 1 to fly by Venus and Mars, respectively, Venera 3 and Mars 2 to impact the respective planet surface, and Venera 7 and Mars 3 to make soft landings on these planets.",
              "administrator": null,
              "founding_year": 1931,
              "launchers": "",
              "spacecraft": "",
              "parent": null,
              "image": {
                "id": 28,
                "name": "[AUTO] Soviet Space Program - image",
                "image_url": "https://thespacedevs-prod.nyc3.digitaloceanspaces.com/media/images/soviet2520space2520program_image_20191229081306.jpeg",
                "thumbnail_url": "https://thespacedevs-prod.nyc3.digitaloceanspaces.com/media/images/255bauto255d__image_thumbnail_20240305184701.jpeg",
                "credit": null,
                "license": {
                  "id": 1,
                  "name": "Unknown",
                  "priority": 9,
                  "link": null
                },
                "single_use": true,
                "variants": []
              },
              "logo": {
                "id": 182,
                "name": "Soviet Space Program logo",
                "image_url": "https://thespacedevs-prod.nyc3.digitaloceanspaces.com/media/images/soviet2520space2520program_logo_20191229081307.png",
                "thumbnail_url": "https://thespacedevs-prod.nyc3.digitaloceanspaces.com/media/images/255bauto255d__image_thumbnail_20240305185129.png",
                "credit": "USSR",
                "license": {
                  "id": 1,
                  "name": "Unknown",
                  "priority": 9,
                  "link": null
                },
                "single_use": true,
                "variants": []
              },
              "social_logo": {
                "id": 92,
                "name": "Soviet Space Program social logo",
                "image_url": "https://thespacedevs-prod.nyc3.digitaloceanspaces.com/media/images/soviet_space_pr_image_20251017113333.png",
                "thumbnail_url": "https://thespacedevs-prod.nyc3.digitaloceanspaces.com/media/images/soviet_space_pr_image_thumbnail_20251017113333.png",
                "credit": "USSR",
                "license": {
                  "id": 1,
                  "name": "Unknown",
                  "priority": 9,
                  "link": null
                },
                "single_use": true,
                "variants": []
              },
              "total_launch_count": 2456,
              "consecutive_successful_launches": 17,
              "successful_launches": 2288,
              "failed_launches": 168,
              "pending_launches": 0,
              "consecutive_successful_landings": 0,
              "successful_landings": 0,
              "failed_landings": 0,
              "attempted_landings": 0,
              "successful_landings_spacecraft": 0,
              "failed_landings_spacecraft": 0,
              "attempted_landings_spacecraft": 0,
              "successful_landings_payload": 3,
              "failed_landings_payload": 0,
              "attempted_landings_payload": 3,
              "info_url": null,
              "wiki_url": "https://en.wikipedia.org/wiki/Soviet_space_program",
              "social_media_links": []
            },
            "image": {
              "id": 2015,
              "name": "Sputnik 1 replica",
              "image_url": "https://thespacedevs-prod.nyc3.digitaloceanspaces.com/media/images/sputnik_1_repli_image_20240317191145.jpeg",
              "thumbnail_url": "https://thespacedevs-prod.nyc3.digitaloceanspaces.com/media/images/sputnik_1_repli_image_thumbnail_20240317191145.jpeg",
              "credit": "Andrey Butko",
              "license": {
                "id": 21,
                "name": "CC BY-SA 3.0",
                "priority": 4,
                "link": "https://creativecommons.org/licenses/by-sa/3.0/deed.en"
              },
              "single_use": true,
              "variants": []
            },
            "wiki_link": "https://en.wikipedia.org/wiki/Sputnik_1",
            "info_link": "",
            "program": [],
            "cost": null,
            "mass": 84.0,
            "description": "Sputnik 1 was the first artificial Earth satellite."
          },
          "landing": null,
          "docking_events": []
        }
      ]
    },
    "mission": {
      "id": 1430,
      "name": "Sputnik 1",
      "type": "Test Flight",
      "description": "First artificial satellite consisting of a 58 cm pressurized aluminium shell containing two 1 W transmitters for a total mass of 83.6 kg.",
      "image": null,
      "orbit": {
        "id": 8,
        "name": "Low Earth Orbit",
        "abbrev": "LEO",
        "celestial_body": {
          "response_mode": "list",
          "id": 1,
          "name": "Earth"
        }
      },
      "agencies": [],
      "info_urls": [],
      "vid_urls": []
    },
    "pad": {
      "id": 32,
      "url": "https://ll.thespacedevs.com/2.3.0/pads/32/?format=json",
      "active": true,
      "agencies": [],
      "name": "1/5",
      "image": null,
      "description": null,
      "info_url": null,
      "wiki_url": "",
      "map_url": "https://www.google.com/maps?q=45.92,63.342",
      "latitude": 45.92,
      "longitude": 63.342,
      "country": {
        "id": 44,
        "name": "Kazakhstan",
        "alpha_2_code": "KZ",
        "alpha_3_code": "KAZ",
        "nationality_name": "Kazakh",
        "nationality_name_composed": "Kazakhstani"
      },
      "map_image": "https://thespacedevs-prod.nyc3.digitaloceanspaces.com/media/map_images/pad_32_20200803143513.jpg",
      "total_launch_count": 487,
      "orbital_launch_attempt_count": 487,
      "fastest_turnaround": "PT23H32M33S",
      "location": {
        "response_mode": "normal",
        "id": 15,
        "url": "https://ll.thespacedevs.com/2.3.0/locations/15/?format=json",
        "name": "Baikonur Cosmodrome, Republic of Kazakhstan",
        "celestial_body": {
          "response_mode": "normal",
          "id": 1,
          "name": "Earth",
          "type": {
            "id": 1,
            "name": "Planet"
          },
          "diameter": 12742000.0,
          "mass": 5.972168e+24,
          "gravity": 9.80655,
          "length_of_day": "1 00:00:00",
          "atmosphere": true,
          "image": {
            "id": 2040,
            "name": "Earth (Apollo 17)",
            "image_url": "https://thespacedevs-prod.nyc3.digitaloceanspaces.com/media/images/earth_2528apol_image_20240402194304.jpeg",
            "thumbnail_url": "https://thespacedevs-prod.nyc3.digitaloceanspaces.com/media/images/earth_2528apol_image_thumbnail_20240402194305.jpeg",
            "credit": "NASA",
            "license": {
              "id": 4,
              "name": "NASA Image and Media Guidelines",
              "priority": 0,
              "link": "https://www.nasa.gov/nasa-brand-center/images-and-media/"
            },
            "single_use": true,
            "variants": []
          },
          "description": "Earth is the third planet from the Sun and the only astronomical object known to harbor life.",
          "wiki_url": "https://en.wikipedia.org/wiki/Earth",
          "total_attempted_launches": 7431,
          "successful_launches": 6878,
          "failed_launches": 553,
          "total_attempted_landings": 1268,
          "successful_landings": 1221,
          "failed_landings": 47
        },
        "active": true,
        "country": {
          "id": 44,
          "name": "Kazakhstan",
          "alpha_2_code": "KZ",
          "alpha_3_code": "KAZ",
          "nationality_name": "Kazakh",
          "nationality_name_composed": "Kazakhstani"
        },
        "description": "The Baikonur Cosmodrome is a spaceport operated by Russia within Kazakhstan. Located in the Kazakh city of Baikonur, it is the largest operational space launch facility in terms of area. All Russian crewed spaceflights are launched from Baikonur.",
        "image": {
          "id": 2198,
          "name": "Soyuz launch pad at the Baikonur Cosmodrome",
          "image_url": "https://thespacedevs-prod.nyc3.digitaloceanspaces.com/media/images/soyuz_launch_pa_image_20240918150530.jpg",
          "thumbnail_url": "https://thespacedevs-prod.nyc3.digitaloceanspaces.com/media/images/soyuz_launch_pa_image_thumbnail_20240918150530.jpeg",
          "credit": "NASA/Bill Ingalls",
          "license": {
            "id": 4,
            "name": "NASA Image and Media Guidelines",
            "priority": 0,
            "link": "https://www.nasa.gov/nasa-brand-center/images-and-media/"
          },
          "single_use": true,
          "variants": []
        },
        "map_image": "https://thespacedevs-prod.nyc3.digitaloceanspaces.com/media/map_images/location_15_20200803142517.jpg",
        "longitude": 63.305,
        "latitude": 45.965,
        "timezone_name": "Asia/Qyzylorda",
        "total_launch_count": 1560,
        "total_landing_count": 0
      }
    },
    "webcast_live": false,
    "program": [],
    "orbital_launch_attempt_count": 1,
    "location_launch_attempt_count": 1,
    "pad_launch_attempt_count": 1,
    "agency_launch_attempt_count": 1,
    "orbital_launch_attempt_count_year": 1,
    "location_launch_attempt_count_year": 1,
    "pad_launch_attempt_count_year": 1,
    "agency_launch_attempt_count_year": 1,
    "flightclub_url": null,
    "updates": [],
    "info_urls": [],
    "vid_urls": [
      {
        "priority": 10,
        "source": "youtube.com",
        "publisher": "SciNews",
        "title": "Sputnik 1 - Earth’s First Artificial Satellite",
        "description": "On 4 October 1957, a Sputnik 8K71PS rocket launched the Earth’s first artificial satellite - Sputnik 1. The satellite was a metal sphere (58 cm in diameter) ...",
        "feature_image": "https://i.ytimg.com/vi/DTDb3eKpPiw/maxresdefault.jpg",
        "url": "https://www.youtube.com/watch?v=DTDb3eKpPiw",
        "type": {
          "id": 1,
          "name": "Official Webcast"
        },
        "language": {
          "id": 1,
          "name": "English",
          "code": "en"
        },
        "start_time": "2017-10-03T13:07:12Z",
        "end_time": "2017-10-03T13:09:19Z",
        "live": false
      }
    ],
    "timeline": [],
    "pad_turnaround": "P0D",
    "mission_patches": []
  }
```

## API Throttle
GET /api-throttle/

#### Purpose
Returns the current rate limit status for the requesting client. This endpoint
does **not** count against the API rate limit, so it can be called freely for
calibration.

#### Return value
status code: 200

Example return value (verified 2026-03-12 against the dev server):
```json
{
  "your_request_limit": 15,
  "limit_frequency_secs": 3600,
  "current_use": 0,
  "next_use_secs": 0,
  "ident": "[ipv4 or api key identifier]"
}
```

| Field | Type | Description |
|---|---|---|
| `your_request_limit` | integer | Maximum requests allowed in the rolling window |
| `limit_frequency_secs` | integer | Rolling window duration in seconds (3600 = 1 hour) |
| `current_use` | integer | Number of requests made in the current window |
| `next_use_secs` | integer | Seconds until the next request slot opens (0 = available now) |
| `ident` | string | The identifier used for rate limiting (IP address or API key) |

Unauthenticated clients get a limit of 15/hour; authenticated clients (with
`Authorization: Token <key>` header) get 30/hour.
